use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

fn default_version() -> u32 {
    1
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppState {
    #[serde(default = "default_version")]
    pub version: u32,
    pub last_check_at: Option<String>,
    pub access_token: Option<String>,
    #[serde(default)]
    pub discovered_tools: Vec<String>,
    #[serde(default)]
    pub providers: HashMap<String, ProviderState>,
    #[serde(default)]
    pub push_subscriptions: Vec<PushSubscription>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            version: default_version(),
            last_check_at: None,
            access_token: None,
            discovered_tools: Vec::new(),
            providers: HashMap::new(),
            push_subscriptions: Vec::new(),
        }
    }
}

impl AppState {
    /// Write state to a .tmp file, fsync, then atomically rename into place.
    pub fn save_atomic(&self, path: &Path) -> Result<()> {
        let tmp_path = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self)?;

        {
            use std::io::Write;
            let mut file = std::fs::File::create(&tmp_path)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
        }

        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Read and parse state from a JSON file.
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let state: AppState = serde_json::from_str(&content)?;
        Ok(state)
    }

    /// Load state, returning default if the file is missing.
    /// If the file exists but is corrupted, back it up to `.corrupted` and return default.
    pub fn load_or_default(path: &Path) -> Self {
        if !path.exists() {
            return AppState::default();
        }

        match AppState::load_from(path) {
            Ok(state) => state,
            Err(err) => {
                warn!("State file corrupted ({}), backing up and using default", err);
                let backup = path.with_extension("json.corrupted");
                if let Err(e) = std::fs::rename(path, &backup) {
                    warn!("Failed to back up corrupted state file: {}", e);
                }
                AppState::default()
            }
        }
    }

    /// Clear `alerts_sent` and `reset_alerts_sent` for any quota window whose
    /// `resets_at` timestamp is in the past.
    pub fn clear_expired_windows(&mut self) {
        let now = Utc::now();
        for provider in self.providers.values_mut() {
            for window in provider.windows.values_mut() {
                if let Some(resets_at_str) = &window.resets_at {
                    if let Ok(resets_at) = chrono::DateTime::parse_from_rfc3339(resets_at_str) {
                        if resets_at < now {
                            window.alerts_sent.clear();
                            window.reset_alerts_sent.clear();
                        }
                    }
                }
            }
        }
    }

    /// Update `last_check_at` to the current UTC time (RFC 3339).
    pub fn mark_checked(&mut self) {
        self.last_check_at = Some(Utc::now().to_rfc3339());
    }

    /// Generate a random UUID access token if one is not already set.
    pub fn ensure_access_token(&mut self) {
        if self.access_token.is_none() {
            self.access_token = Some(Uuid::new_v4().to_string());
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderState {
    #[serde(default)]
    pub windows: HashMap<String, QuotaWindowState>,
    /// Last error message from quota fetch (cleared on success).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuotaWindowState {
    #[serde(default)]
    pub utilization: f64,
    pub used_tokens: Option<i64>,
    pub used_count: Option<u64>,
    pub resets_at: Option<String>,
    #[serde(default)]
    pub alerts_sent: Vec<u32>,
    #[serde(default)]
    pub reset_alerts_sent: Vec<u32>,
}

impl Default for QuotaWindowState {
    fn default() -> Self {
        Self {
            utilization: 0.0,
            used_tokens: None,
            used_count: None,
            resets_at: None,
            alerts_sent: Vec::new(),
            reset_alerts_sent: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PushSubscription {
    pub endpoint: String,
    pub keys: PushKeys,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PushKeys {
    pub p256dh: String,
    pub auth: String,
}
