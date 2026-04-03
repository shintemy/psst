use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub thresholds: ThresholdConfig,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub server: ServerConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            thresholds: ThresholdConfig::default(),
            providers: HashMap::new(),
            notifications: NotificationConfig::default(),
            server: ServerConfig::default(),
        }
    }
}

impl Config {
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn default_config_toml() -> &'static str {
        r#"# Psst - AI coding tool usage monitor & notifier
# Default configuration

[general]
# How often to check usage (in minutes)
check_interval_minutes = 20
# Automatically discover AI coding tools
auto_discover = true

[thresholds]
# Usage percentage alerts (0-100)
usage_alerts = [50, 80]
# Hours before reset to send alerts
reset_alerts_hours = [24, 12, 1]
# Skip reset alert if usage is above this fraction (0.0-1.0)
skip_reset_alert_above = 0.95

[notifications]
# Enable desktop notifications
desktop = true
# Quiet hours range (e.g. "23:00-08:00"), leave unset to disable
# quiet_hours = "23:00-08:00"

[notifications.telegram]
enabled = false
bot_token = ""
chat_id = ""

[notifications.serverchan]
enabled = false
send_key = ""

[notifications.web_push]
enabled = true

[server]
# Address to bind the web UI server
bind = "127.0.0.1:3377"
"#
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GeneralConfig {
    #[serde(default = "default_check_interval_minutes")]
    pub check_interval_minutes: u32,
    #[serde(default = "default_true")]
    pub auto_discover: bool,
}

fn default_check_interval_minutes() -> u32 {
    20
}

fn default_true() -> bool {
    true
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            check_interval_minutes: default_check_interval_minutes(),
            auto_discover: true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ThresholdConfig {
    #[serde(default = "default_usage_alerts")]
    pub usage_alerts: Vec<u32>,
    #[serde(default = "default_reset_alerts_hours")]
    pub reset_alerts_hours: Vec<u32>,
    #[serde(default = "default_skip_reset_alert_above")]
    pub skip_reset_alert_above: f64,
}

fn default_usage_alerts() -> Vec<u32> {
    vec![50, 80]
}

fn default_reset_alerts_hours() -> Vec<u32> {
    vec![24, 12, 1]
}

fn default_skip_reset_alert_above() -> f64 {
    0.95
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        Self {
            usage_alerts: default_usage_alerts(),
            reset_alerts_hours: default_reset_alerts_hours(),
            skip_reset_alert_above: default_skip_reset_alert_above(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ProviderConfig {
    pub monthly_fast_requests: Option<u64>,
    pub billing_day: Option<u32>,
    pub daily_token_limit: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub desktop: bool,
    pub quiet_hours: Option<String>,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub serverchan: ServerChanConfig,
    #[serde(default)]
    pub web_push: WebPushConfig,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            desktop: true,
            quiet_hours: None,
            telegram: TelegramConfig::default(),
            serverchan: ServerChanConfig::default(),
            web_push: WebPushConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub chat_id: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ServerChanConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub send_key: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WebPushConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for WebPushConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
}

fn default_bind() -> String {
    "127.0.0.1:3377".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
        }
    }
}
