pub mod desktop;
pub mod telegram;
pub mod serverchan;
pub mod web_push_notifier;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;

use crate::threshold::{AlertEvent, AlertKind};

/// A formatted notification ready to send.
#[derive(Debug, Clone)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub provider_id: String,
    pub window_name: String,
}

/// Trait for all notification backends.
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, notification: &Notification) -> Result<()>;
    fn name(&self) -> &str;
    fn is_enabled(&self) -> bool;
}

/// Dispatches a notification to all enabled notifiers.
pub struct Dispatcher {
    notifiers: Vec<Box<dyn Notifier>>,
}

impl Dispatcher {
    pub fn new(notifiers: Vec<Box<dyn Notifier>>) -> Self {
        Self { notifiers }
    }

    pub async fn dispatch(&self, notification: &Notification) {
        for notifier in &self.notifiers {
            if notifier.is_enabled() {
                match notifier.send(notification).await {
                    Ok(()) => {
                        tracing::info!(
                            notifier = notifier.name(),
                            title = notification.title.as_str(),
                            "Notification sent"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            notifier = notifier.name(),
                            title = notification.title.as_str(),
                            error = %e,
                            "Failed to send notification"
                        );
                    }
                }
            }
        }
    }
}

/// Map window name key to human-readable Chinese label.
fn window_display_name(window_name: &str) -> &str {
    match window_name {
        "five_hour" => "5小时窗口",
        "seven_day" => "7天窗口",
        "monthly" | "monthly_requests" => "月度配额",
        "auto_requests" => "Auto模型配额",
        "api_requests" => "API模型配额",
        "weekly" | "weekly_requests" => "周预算(估算)",
        "daily" | "daily_requests" => "日预算(估算)",
        "daily_tokens" => "日配额",
        other => other,
    }
}

/// Map provider_id to display name.
fn provider_display_name(provider_id: &str) -> String {
    let mut chars = provider_id.chars();
    match chars.next() {
        None => provider_id.to_string(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Format a duration (in minutes) into a human-readable Chinese time string.
fn format_duration_minutes(total_minutes: i64) -> String {
    if total_minutes >= 24 * 60 {
        let days = total_minutes / (24 * 60);
        format!("{}天后", days)
    } else if total_minutes >= 60 {
        let hours = total_minutes / 60;
        let mins = total_minutes % 60;
        if mins == 0 {
            format!("{}小时后", hours)
        } else {
            format!("{}小时{}分钟后", hours, mins)
        }
    } else {
        format!("{}分钟后", total_minutes)
    }
}

/// Format an `AlertEvent` into a `Notification`.
pub fn format_notification(event: &AlertEvent) -> Notification {
    let provider = provider_display_name(&event.provider_id);
    let window = window_display_name(&event.window_name);
    let used_pct = (event.utilization * 100.0).round() as u32;
    let remaining_pct = 100u32.saturating_sub(used_pct);

    // Format reset time string if available
    let reset_time_str = event.resets_at.map(|resets_at| {
        let now = Utc::now();
        let remaining_minutes = (resets_at - now).num_minutes().max(0);
        format_duration_minutes(remaining_minutes)
    });

    // Build the usage detail line — handle over-100% gracefully.
    let usage_line = if used_pct > 100 {
        format!("当前使用率: {}%（已超出配额）", used_pct)
    } else {
        format!("当前使用率: {}%\n剩余: {}%", used_pct, remaining_pct)
    };

    match &event.kind {
        AlertKind::UsageThreshold(_pct) => {
            let title = format!("Psst! {} {}已用 {}%", provider, window, used_pct);
            let mut body = usage_line;
            if let Some(reset_str) = reset_time_str {
                body.push_str(&format!("\n将在{}重置", reset_str));
            }
            Notification {
                title,
                body,
                provider_id: event.provider_id.clone(),
                window_name: event.window_name.clone(),
            }
        }
        AlertKind::ResetCountdown(hours) => {
            let countdown_label = if *hours >= 24 {
                let days = hours / 24;
                format!("{}天", days)
            } else {
                format!("{}小时", hours)
            };
            let title = format!("Psst! {} {} {}后重置", provider, window, countdown_label);
            let mut body = usage_line;
            if remaining_pct > 10 {
                body.push_str("\n建议在重置前充分利用剩余额度");
            }
            if let Some(reset_str) = reset_time_str {
                body.push_str(&format!("\n将在{}重置", reset_str));
            }
            Notification {
                title,
                body,
                provider_id: event.provider_id.clone(),
                window_name: event.window_name.clone(),
            }
        }
    }
}
