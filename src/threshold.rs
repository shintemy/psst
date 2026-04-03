use chrono::{DateTime, Utc};

use crate::state::QuotaWindowState;

/// The kind of alert triggered.
#[derive(Debug, Clone, PartialEq)]
pub enum AlertKind {
    /// Triggered when utilization crosses a percentage threshold (0–100).
    UsageThreshold(u32),
    /// Triggered when the window will reset within the given number of hours.
    ResetCountdown(u32),
}

/// A single alert event produced by `evaluate_thresholds`.
#[derive(Debug, Clone)]
pub struct AlertEvent {
    pub provider_id: String,
    pub window_name: String,
    pub kind: AlertKind,
    pub utilization: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

/// Evaluate which alerts should fire for a quota window.
///
/// # Arguments
/// - `provider_id` / `window_name` — identifiers embedded in every `AlertEvent`
/// - `window` — current window state
/// - `usage_alerts` — percentage thresholds (0–100); fire when `utilization >= threshold/100`
///   and that threshold is not already in `alerts_sent`
/// - `reset_alerts_hours` — countdown thresholds in hours; fire when
///   `remaining_minutes <= hours * 60` and that value is not in `reset_alerts_sent`
///   and `utilization < skip_reset_alert_above`
/// - `skip_reset_alert_above` — suppress reset-countdown alerts when utilization is
///   at or above this value (e.g. 0.95)
pub fn evaluate_thresholds(
    provider_id: &str,
    window_name: &str,
    window: &QuotaWindowState,
    usage_alerts: &[u32],
    reset_alerts_hours: &[u32],
    skip_reset_alert_above: f64,
) -> Vec<AlertEvent> {
    let mut events: Vec<AlertEvent> = Vec::new();

    // Parse resets_at once
    let resets_at: Option<DateTime<Utc>> = window
        .resets_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    // Rule A: Usage threshold alerts
    for &threshold in usage_alerts {
        let threshold_fraction = threshold as f64 / 100.0;
        if window.utilization >= threshold_fraction && !window.alerts_sent.contains(&threshold) {
            events.push(AlertEvent {
                provider_id: provider_id.to_string(),
                window_name: window_name.to_string(),
                kind: AlertKind::UsageThreshold(threshold),
                utilization: window.utilization,
                resets_at,
            });
        }
    }

    // Rule B: Reset countdown alerts
    if window.utilization < skip_reset_alert_above {
        if let Some(resets_at_dt) = resets_at {
            let now = Utc::now();
            let remaining_minutes = (resets_at_dt - now).num_minutes();

            for &hours in reset_alerts_hours {
                let threshold_minutes = hours as i64 * 60;
                if remaining_minutes <= threshold_minutes
                    && !window.reset_alerts_sent.contains(&hours)
                {
                    events.push(AlertEvent {
                        provider_id: provider_id.to_string(),
                        window_name: window_name.to_string(),
                        kind: AlertKind::ResetCountdown(hours),
                        utilization: window.utilization,
                        resets_at,
                    });
                }
            }
        }
    }

    events
}

/// Record which alerts fired by pushing their identifiers into the window state.
///
/// - `AlertKind::UsageThreshold(t)` → appended to `alerts_sent`
/// - `AlertKind::ResetCountdown(h)` → appended to `reset_alerts_sent`
pub fn record_alerts(window: &mut QuotaWindowState, events: &[AlertEvent]) {
    for event in events {
        match event.kind {
            AlertKind::UsageThreshold(threshold) => {
                if !window.alerts_sent.contains(&threshold) {
                    window.alerts_sent.push(threshold);
                }
            }
            AlertKind::ResetCountdown(hours) => {
                if !window.reset_alerts_sent.contains(&hours) {
                    window.reset_alerts_sent.push(hours);
                }
            }
        }
    }
}
