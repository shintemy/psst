use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::data_sources::claude_quota::ClaudeQuotaProvider;
use crate::data_sources::discovery::discover_tools;
use crate::data_sources::estimated_quota::EstimatedQuotaProvider;
use crate::data_sources::QuotaProvider;
use crate::notifiers::{format_notification, Dispatcher};
use crate::state::AppState;
use crate::threshold::{evaluate_thresholds, record_alerts};

pub struct Scheduler {
    config: Config,
    state_path: PathBuf,
    state: Arc<Mutex<AppState>>,
    dispatcher: Arc<Dispatcher>,
    home_dir: String,
}

impl Scheduler {
    pub fn new(
        config: Config,
        state_path: PathBuf,
        state: Arc<Mutex<AppState>>,
        dispatcher: Dispatcher,
        home_dir: String,
    ) -> Self {
        Self {
            config,
            state_path,
            state,
            dispatcher: Arc::new(dispatcher),
            home_dir,
        }
    }

    /// Run the check loop: run once immediately, then repeat every check_interval_minutes.
    pub async fn run(&self) {
        let interval_mins = self.config.general.check_interval_minutes;
        info!("Scheduler starting — checking every {} minutes", interval_mins);

        self.check_once().await;

        let interval = tokio::time::Duration::from_secs(interval_mins as u64 * 60);
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await; // consume the first tick (fires immediately)
        loop {
            ticker.tick().await;
            self.check_once().await;
        }
    }

    /// Perform one full quota check cycle.
    pub async fn check_once(&self) {
        info!("Running quota check");

        let providers = self.build_providers();

        // Collect all quota data before acquiring the state lock.
        let mut quota_results = Vec::new();
        for provider in &providers {
            match provider.fetch_quota().await {
                Ok(info) => quota_results.push(info),
                Err(e) => {
                    warn!(provider = provider.provider_id(), error = %e, "Failed to fetch quota");
                }
            }
        }

        let mut state = self.state.lock().await;
        state.clear_expired_windows();

        // Auto-discover tools if enabled.
        if self.config.general.auto_discover {
            let discovered = discover_tools(&self.home_dir);
            info!("Discovered tools: {:?}", discovered);
            state.discovered_tools = discovered;
        }

        // Process each provider's quota info.
        for quota_info in quota_results {
            let provider_id = &quota_info.provider_id;
            let provider_state = state
                .providers
                .entry(provider_id.clone())
                .or_default();

            for window in &quota_info.windows {
                let window_state = provider_state
                    .windows
                    .entry(window.name.clone())
                    .or_default();

                // Update window state from fresh quota data.
                window_state.utilization = window.utilization;
                window_state.resets_at = window.resets_at.map(|dt| dt.to_rfc3339());
                window_state.used_tokens = window.used_tokens;
                window_state.used_count = window.used_count;

                // Evaluate thresholds and collect events.
                let events = evaluate_thresholds(
                    provider_id,
                    &window.name,
                    window_state,
                    &self.config.thresholds.usage_alerts,
                    &self.config.thresholds.reset_alerts_hours,
                    self.config.thresholds.skip_reset_alert_above,
                );

                if !events.is_empty() {
                    info!(
                        provider = provider_id.as_str(),
                        window = window.name.as_str(),
                        count = events.len(),
                        "Threshold events to dispatch"
                    );
                }

                // Dispatch notifications (outside the lock to avoid holding it during I/O).
                // We snapshot the events here and dispatch after releasing the lock below.
                // For now dispatch while holding lock — notifiers are async and fast in practice.
                for event in &events {
                    let notification = format_notification(event);
                    self.dispatcher.dispatch(&notification).await;
                }

                record_alerts(window_state, &events);
            }
        }

        state.mark_checked();

        if let Err(e) = state.save_atomic(&self.state_path) {
            error!(error = %e, "Failed to save state");
        } else {
            info!("State saved to {}", self.state_path.display());
        }
    }

    /// Build the list of providers based on config and discovered tools.
    pub fn build_providers(&self) -> Vec<Box<dyn QuotaProvider>> {
        let mut providers: Vec<Box<dyn QuotaProvider>> = Vec::new();

        // Claude is always checked if it appears in config or as a discovered tool.
        let has_claude_config = self.config.providers.contains_key("claude");
        let state_ref = &self.home_dir;

        // We always try Claude (it will fail gracefully if credentials aren't present).
        if has_claude_config || self.config.general.auto_discover {
            providers.push(Box::new(ClaudeQuotaProvider::new(state_ref.clone())));
        }

        // Add other configured providers as EstimatedQuotaProvider.
        for (id, provider_config) in &self.config.providers {
            if id == "claude" {
                continue;
            }
            // Only include if limits are configured.
            if provider_config.monthly_fast_requests.is_some()
                || provider_config.daily_token_limit.is_some()
            {
                providers.push(Box::new(EstimatedQuotaProvider::new(
                    id.clone(),
                    self.home_dir.clone(),
                    provider_config.clone(),
                )));
            }
        }

        providers
    }

    /// Return a clone of the shared state handle (for use by the web server).
    pub fn shared_state(&self) -> Arc<Mutex<AppState>> {
        Arc::clone(&self.state)
    }
}
