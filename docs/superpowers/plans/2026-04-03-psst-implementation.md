# Psst Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a local daemon that monitors AI coding tool usage quotas and sends multi-channel notifications at configurable thresholds.

**Architecture:** Rust binary depending on tokscale-core for data parsing. Scheduler loop checks usage every 20 minutes, threshold engine evaluates alert rules, dispatcher fans out to notification channels. Axum HTTP server provides PWA dashboard and push subscription endpoint. State persisted atomically to JSON.

**Tech Stack:** Rust 1.94+, tokio, tokscale-core (local path dep), axum 0.8, reqwest, web-push 0.11, notify-rust 4.12, clap 4, serde, chrono

**Spec:** `docs/superpowers/specs/2026-04-03-psst-usage-notifier-design.md`

---

## File Structure

```
Psst/
├── Cargo.toml
├── src/
│   ├── main.rs                      # CLI entry (clap): init, run, status, install, uninstall
│   ├── config.rs                    # Load/validate config.toml
│   ├── state.rs                     # Atomic read/write of state.json
│   ├── scheduler.rs                 # Main check loop (tokio interval)
│   ├── data_sources/
│   │   ├── mod.rs                   # QuotaProvider trait + aggregate fetch
│   │   ├── discovery.rs             # Auto-discover installed AI tools via tokscale scanner
│   │   ├── usage_collector.rs       # Collect token usage from tokscale-core parsers
│   │   ├── claude_quota.rs          # Claude OAuth API for precise quota
│   │   └── estimated_quota.rs       # Generic quota estimation (limit - consumed)
│   ├── threshold.rs                 # Threshold engine: usage alerts + reset countdown
│   ├── notifiers/
│   │   ├── mod.rs                   # Notifier trait + Dispatcher
│   │   ├── desktop.rs               # macOS Notification Center via notify-rust
│   │   ├── telegram.rs              # Telegram Bot API
│   │   ├── serverchan.rs            # Server酱 (WeChat push)
│   │   └── web_push_notifier.rs     # PWA Web Push via web-push crate
│   └── web/
│       ├── mod.rs                   # Axum router + server startup
│       ├── api.rs                   # REST endpoints: /api/status, /api/subscribe, /api/health
│       └── static/
│           ├── index.html           # PWA dashboard
│           ├── manifest.json        # PWA manifest
│           ├── sw.js                # Service Worker for push
│           └── app.js               # Frontend logic
├── tests/
│   ├── config_test.rs
│   ├── state_test.rs
│   ├── threshold_test.rs
│   ├── discovery_test.rs
│   └── notifier_test.rs
```

---

### Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

- [ ] **Step 1: Create Cargo.toml with all dependencies**

```toml
[package]
name = "psst"
version = "0.1.0"
edition = "2021"
description = "AI coding tool usage monitor & notifier"
license = "MIT"

[dependencies]
tokscale-core = { path = "tokscale-main/crates/tokscale-core" }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
axum = "0.8"
tower-http = { version = "0.6", features = ["fs", "cors"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
clap = { version = "4", features = ["derive"] }
notify-rust = "4"
web-push = "0.11"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"
anyhow = "1"
async-trait = "0.1"
uuid = { version = "1", features = ["v4"] }
dirs = "6"
```

- [ ] **Step 2: Create minimal main.rs**

```rust
fn main() {
    println!("Psst - AI coding tool usage monitor");
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles with no errors (warnings OK).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "feat: scaffold project with dependencies"
```

---

### Task 2: Config Module

**Files:**
- Create: `src/config.rs`
- Create: `tests/config_test.rs`

- [ ] **Step 1: Write the failing test for config parsing**

Create `tests/config_test.rs`:

```rust
use std::collections::HashMap;

// We'll test config parsing from a TOML string
#[test]
fn test_parse_default_config() {
    let config = psst::config::Config::default();
    assert_eq!(config.general.check_interval_minutes, 20);
    assert!(config.general.auto_discover);
    assert_eq!(config.thresholds.usage_alerts, vec![50, 80]);
    assert_eq!(config.thresholds.reset_alerts_hours, vec![24, 12, 1]);
    assert!((config.thresholds.skip_reset_alert_above - 0.95).abs() < f64::EPSILON);
}

#[test]
fn test_parse_config_from_toml() {
    let toml_str = r#"
[general]
check_interval_minutes = 10
auto_discover = false

[thresholds]
usage_alerts = [30, 60, 90]
reset_alerts_hours = [48, 24]
skip_reset_alert_above = 0.90

[providers.claude]

[providers.cursor]
monthly_fast_requests = 500
billing_day = 15

[providers.codex]
daily_token_limit = 1000000

[notifications]
desktop = true
quiet_hours = "23:00-08:00"

[notifications.telegram]
enabled = true
bot_token = "123:ABC"
chat_id = "456"

[notifications.serverchan]
enabled = false
send_key = ""

[notifications.web_push]
enabled = true

[server]
bind = "0.0.0.0:3377"
"#;
    let config: psst::config::Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.general.check_interval_minutes, 10);
    assert!(!config.general.auto_discover);
    assert_eq!(config.thresholds.usage_alerts, vec![30, 60, 90]);
    assert_eq!(config.providers.get("cursor").unwrap().monthly_fast_requests, Some(500));
    assert_eq!(config.providers.get("cursor").unwrap().billing_day, Some(15));
    assert_eq!(config.providers.get("codex").unwrap().daily_token_limit, Some(1000000));
    assert!(config.providers.contains_key("claude"));
    assert!(config.notifications.desktop);
    assert_eq!(config.notifications.quiet_hours, Some("23:00-08:00".to_string()));
    assert!(config.notifications.telegram.enabled);
    assert_eq!(config.notifications.telegram.bot_token, "123:ABC");
    assert_eq!(config.server.bind, "0.0.0.0:3377");
}

#[test]
fn test_config_load_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, r#"
[general]
check_interval_minutes = 5

[thresholds]
usage_alerts = [80]
reset_alerts_hours = [1]

[notifications]
desktop = false

[server]
bind = "127.0.0.1:4000"
"#).unwrap();

    let config = psst::config::Config::load_from(&path).unwrap();
    assert_eq!(config.general.check_interval_minutes, 5);
    assert!(!config.notifications.desktop);
    assert_eq!(config.server.bind, "127.0.0.1:4000");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test config_test 2>&1 | tail -5`
Expected: FAIL — module `config` not found.

- [ ] **Step 3: Implement config.rs**

Create `src/config.rs`:

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GeneralConfig {
    #[serde(default = "default_interval")]
    pub check_interval_minutes: u32,
    #[serde(default = "default_true")]
    pub auto_discover: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ThresholdConfig {
    #[serde(default = "default_usage_alerts")]
    pub usage_alerts: Vec<u32>,
    #[serde(default = "default_reset_alerts")]
    pub reset_alerts_hours: Vec<u32>,
    #[serde(default = "default_skip_above")]
    pub skip_reset_alert_above: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ProviderConfig {
    #[serde(default)]
    pub monthly_fast_requests: Option<u64>,
    #[serde(default)]
    pub billing_day: Option<u32>,
    #[serde(default)]
    pub daily_token_limit: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub desktop: bool,
    #[serde(default)]
    pub quiet_hours: Option<String>,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub serverchan: ServerChanConfig,
    #[serde(default)]
    pub web_push: WebPushConfig,
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
}

// Default value functions
fn default_interval() -> u32 { 20 }
fn default_true() -> bool { true }
fn default_usage_alerts() -> Vec<u32> { vec![50, 80] }
fn default_reset_alerts() -> Vec<u32> { vec![24, 12, 1] }
fn default_skip_above() -> f64 { 0.95 }
fn default_bind() -> String { "127.0.0.1:3377".to_string() }

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

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            check_interval_minutes: default_interval(),
            auto_discover: true,
        }
    }
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        Self {
            usage_alerts: default_usage_alerts(),
            reset_alerts_hours: default_reset_alerts(),
            skip_reset_alert_above: default_skip_above(),
        }
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            desktop: true,
            quiet_hours: None,
            telegram: TelegramConfig::default(),
            serverchan: ServerChanConfig::default(),
            web_push: WebPushConfig { enabled: true },
        }
    }
}

impl Default for WebPushConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { bind: default_bind() }
    }
}

impl Config {
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| "Failed to parse config TOML")?;
        Ok(config)
    }

    pub fn default_config_toml() -> &'static str {
        r#"[general]
check_interval_minutes = 20          # Check interval in minutes
auto_discover = true                 # Auto-discover installed AI tools

[thresholds]
usage_alerts = [50, 80]              # Alert when usage reaches these percentages
reset_alerts_hours = [24, 12, 1]     # Alert this many hours before reset
skip_reset_alert_above = 0.95        # Don't send reset alerts above this utilization

# -- Tier 1: Precise quota (tools with APIs) --

[providers.claude]
# Auto-reads ~/.claude/.credentials.json, no config needed

[providers.cursor]
# monthly_fast_requests = 500        # Your plan's limit
# billing_day = 15                   # Day of month when quota resets

# -- Tier 2: Custom quota (configure limits for any tool) --

# [providers.codex]
# daily_token_limit = 1000000

# [providers.gemini]
# daily_token_limit = 500000

# -- Notification channels --

[notifications]
desktop = true
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
bind = "127.0.0.1:3377"
# bind = "0.0.0.0:3377"             # Uncomment for LAN access
"#
    }
}
```

Update `src/main.rs` to expose the module:

```rust
pub mod config;

fn main() {
    println!("Psst - AI coding tool usage monitor");
}
```

- [ ] **Step 4: Add tempfile dev dependency to Cargo.toml**

Add under `[dev-dependencies]`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test config_test -- --nocapture 2>&1 | tail -10`
Expected: All 3 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/config.rs src/main.rs tests/config_test.rs Cargo.toml Cargo.lock
git commit -m "feat: add config module with TOML parsing and defaults"
```

---

### Task 3: State Module

**Files:**
- Create: `src/state.rs`
- Create: `tests/state_test.rs`

- [ ] **Step 1: Write failing tests for state persistence**

Create `tests/state_test.rs`:

```rust
use psst::state::{AppState, ProviderState, QuotaWindowState};
use chrono::Utc;

#[test]
fn test_default_state() {
    let state = AppState::default();
    assert_eq!(state.version, 1);
    assert!(state.providers.is_empty());
    assert!(state.push_subscriptions.is_empty());
    assert!(state.discovered_tools.is_empty());
}

#[test]
fn test_state_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");

    let mut state = AppState::default();
    state.discovered_tools = vec!["claude".to_string(), "cursor".to_string()];

    let mut window = QuotaWindowState::default();
    window.utilization = 0.65;
    window.resets_at = Some("2026-04-03T14:00:00Z".to_string());
    window.alerts_sent.push(50);

    let mut provider = ProviderState::default();
    provider.windows.insert("five_hour".to_string(), window);
    state.providers.insert("claude".to_string(), provider);

    // Write
    state.save_atomic(&path).unwrap();

    // Read back
    let loaded = AppState::load_from(&path).unwrap();
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.discovered_tools, vec!["claude", "cursor"]);
    let claude = loaded.providers.get("claude").unwrap();
    let five_hour = claude.windows.get("five_hour").unwrap();
    assert!((five_hour.utilization - 0.65).abs() < f64::EPSILON);
    assert_eq!(five_hour.alerts_sent, vec![50]);
}

#[test]
fn test_state_load_missing_file_returns_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonexistent.json");
    let state = AppState::load_or_default(&path);
    assert_eq!(state.version, 1);
    assert!(state.providers.is_empty());
}

#[test]
fn test_state_load_corrupted_file_returns_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, "NOT VALID JSON {{{").unwrap();
    let state = AppState::load_or_default(&path);
    assert_eq!(state.version, 1);
    // Corrupted file should be backed up
    assert!(dir.path().join("state.json.corrupted").exists());
}

#[test]
fn test_reset_expired_windows() {
    let mut state = AppState::default();

    // Create a window that expired in the past
    let mut window = QuotaWindowState::default();
    window.utilization = 0.80;
    window.resets_at = Some("2020-01-01T00:00:00Z".to_string());
    window.alerts_sent = vec![50, 80];
    window.reset_alerts_sent = vec![24, 12];

    let mut provider = ProviderState::default();
    provider.windows.insert("five_hour".to_string(), window);
    state.providers.insert("claude".to_string(), provider);

    state.clear_expired_windows();

    let claude = state.providers.get("claude").unwrap();
    let five_hour = claude.windows.get("five_hour").unwrap();
    assert!(five_hour.alerts_sent.is_empty());
    assert!(five_hour.reset_alerts_sent.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test state_test 2>&1 | tail -5`
Expected: FAIL — module `state` not found.

- [ ] **Step 3: Implement state.rs**

Create `src/state.rs`:

```rust
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppState {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub last_check_at: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub discovered_tools: Vec<String>,
    #[serde(default)]
    pub providers: HashMap<String, ProviderState>,
    #[serde(default)]
    pub push_subscriptions: Vec<PushSubscription>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderState {
    #[serde(default)]
    pub windows: HashMap<String, QuotaWindowState>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuotaWindowState {
    #[serde(default)]
    pub utilization: f64,
    #[serde(default)]
    pub used_tokens: Option<i64>,
    #[serde(default)]
    pub used_count: Option<u64>,
    #[serde(default)]
    pub resets_at: Option<String>,
    #[serde(default)]
    pub alerts_sent: Vec<u32>,
    #[serde(default)]
    pub reset_alerts_sent: Vec<u32>,
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

fn default_version() -> u32 { 1 }

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

impl Default for AppState {
    fn default() -> Self {
        Self {
            version: 1,
            last_check_at: None,
            access_token: None,
            discovered_tools: Vec::new(),
            providers: HashMap::new(),
            push_subscriptions: Vec::new(),
        }
    }
}

impl AppState {
    /// Atomic save: write to .tmp, fsync, rename
    pub fn save_atomic(&self, path: &Path) -> Result<()> {
        let tmp_path = path.with_extension("json.tmp");

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize state")?;

        let mut file = std::fs::File::create(&tmp_path)
            .with_context(|| format!("Failed to create temp file: {}", tmp_path.display()))?;
        file.write_all(json.as_bytes())
            .context("Failed to write state")?;
        file.sync_all()
            .context("Failed to fsync state file")?;

        std::fs::rename(&tmp_path, path)
            .with_context(|| format!("Failed to rename {} -> {}", tmp_path.display(), path.display()))?;

        Ok(())
    }

    /// Load state from file
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read state: {}", path.display()))?;
        let state: AppState = serde_json::from_str(&content)
            .with_context(|| "Failed to parse state JSON")?;
        Ok(state)
    }

    /// Load state or return default (handles missing + corrupted files)
    pub fn load_or_default(path: &Path) -> Self {
        match Self::load_from(path) {
            Ok(state) => state,
            Err(e) => {
                if path.exists() {
                    // File exists but corrupted — back it up
                    let backup = path.with_extension("json.corrupted");
                    let _ = std::fs::rename(path, &backup);
                    tracing::warn!(
                        "State file corrupted, backed up to {}: {}",
                        backup.display(),
                        e
                    );
                }
                Self::default()
            }
        }
    }

    /// Clear alerts for windows whose resets_at has passed
    pub fn clear_expired_windows(&mut self) {
        let now = Utc::now();
        for provider in self.providers.values_mut() {
            for window in provider.windows.values_mut() {
                if let Some(ref resets_at) = window.resets_at {
                    if let Ok(reset_time) = resets_at.parse::<chrono::DateTime<Utc>>() {
                        if now > reset_time {
                            window.alerts_sent.clear();
                            window.reset_alerts_sent.clear();
                            window.utilization = 0.0;
                            window.used_tokens = None;
                            window.used_count = None;
                        }
                    }
                }
            }
        }
    }

    /// Update last_check_at to now
    pub fn mark_checked(&mut self) {
        self.last_check_at = Some(Utc::now().to_rfc3339());
    }

    /// Generate access token if not set
    pub fn ensure_access_token(&mut self) {
        if self.access_token.is_none() {
            self.access_token = Some(uuid::Uuid::new_v4().to_string());
        }
    }
}
```

Update `src/main.rs`:

```rust
pub mod config;
pub mod state;

fn main() {
    println!("Psst - AI coding tool usage monitor");
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test state_test -- --nocapture 2>&1 | tail -10`
Expected: All 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/state.rs tests/state_test.rs src/main.rs
git commit -m "feat: add state module with atomic persistence and recovery"
```

---

### Task 4: Auto-Discovery

**Files:**
- Create: `src/data_sources/mod.rs`
- Create: `src/data_sources/discovery.rs`
- Create: `tests/discovery_test.rs`

- [ ] **Step 1: Write failing test**

Create `tests/discovery_test.rs`:

```rust
use psst::data_sources::discovery::discover_tools;

#[test]
fn test_discover_finds_claude_if_dir_exists() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();

    // Create Claude projects dir with a JSONL file
    let claude_dir = home.join(".claude").join("projects").join("test");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("conversation.jsonl"), "{}").unwrap();

    let tools = discover_tools(home.to_str().unwrap());
    assert!(tools.contains(&"claude".to_string()));
}

#[test]
fn test_discover_returns_empty_for_clean_home() {
    let dir = tempfile::tempdir().unwrap();
    let tools = discover_tools(dir.path().to_str().unwrap());
    assert!(tools.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test discovery_test 2>&1 | tail -5`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement discovery module**

Create `src/data_sources/mod.rs`:

```rust
pub mod discovery;
pub mod usage_collector;
pub mod claude_quota;
pub mod estimated_quota;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Information about a single quota window (e.g. "five_hour", "monthly")
#[derive(Debug, Clone)]
pub struct QuotaWindow {
    pub name: String,
    pub utilization: f64,
    pub resets_at: Option<DateTime<Utc>>,
    pub used_tokens: Option<i64>,
    pub used_count: Option<u64>,
}

/// Result from a quota provider
#[derive(Debug, Clone)]
pub struct QuotaInfo {
    pub provider_id: String,
    pub windows: Vec<QuotaWindow>,
}

/// Trait for fetching quota information
#[async_trait]
pub trait QuotaProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn fetch_quota(&self) -> Result<QuotaInfo>;
}
```

Create `src/data_sources/discovery.rs`:

```rust
use tokscale_core::clients::ClientId;
use tokscale_core::scanner::scan_all_clients;

/// Discover which AI coding tools are installed by scanning for their data files.
/// Returns a list of tool IDs (e.g. ["claude", "cursor", "codex"]).
pub fn discover_tools(home_dir: &str) -> Vec<String> {
    let all_client_ids: Vec<String> = ClientId::iter()
        .map(|c| c.as_str().to_string())
        .collect();

    let scan_result = scan_all_clients(home_dir, &all_client_ids);

    let mut found = Vec::new();
    for client_id in ClientId::iter() {
        let files = scan_result.get(client_id);
        if !files.is_empty() {
            found.push(client_id.as_str().to_string());
        }
    }

    // Also check special DB sources
    if scan_result.opencode_db.is_some() && !found.contains(&"opencode".to_string()) {
        found.push("opencode".to_string());
    }
    if scan_result.kilo_db.is_some() && !found.contains(&"kilo".to_string()) {
        found.push("kilo".to_string());
    }

    found
}
```

Update `src/main.rs`:

```rust
pub mod config;
pub mod data_sources;
pub mod state;

fn main() {
    println!("Psst - AI coding tool usage monitor");
}
```

- [ ] **Step 4: Create stubs for other data_sources submodules**

Create `src/data_sources/usage_collector.rs`:
```rust
// Usage collector — will be implemented in Task 5
```

Create `src/data_sources/claude_quota.rs`:
```rust
// Claude quota provider — will be implemented in Task 6
```

Create `src/data_sources/estimated_quota.rs`:
```rust
// Estimated quota provider — will be implemented in Task 7
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test discovery_test -- --nocapture 2>&1 | tail -10`
Expected: Both tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/data_sources/ tests/discovery_test.rs src/main.rs
git commit -m "feat: add auto-discovery of installed AI tools via tokscale scanner"
```

---

### Task 5: Usage Collector

**Files:**
- Modify: `src/data_sources/usage_collector.rs`

- [ ] **Step 1: Implement usage_collector.rs**

This module wraps tokscale-core to collect token usage for a given tool and time window.

```rust
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use tokscale_core::{LocalParseOptions, UnifiedMessage};

/// Collected usage summary for a tool within a time window.
#[derive(Debug, Clone)]
pub struct UsageSummary {
    pub tool_id: String,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub message_count: i32,
    pub window_start: DateTime<Utc>,
}

/// Collect usage for a specific tool since a given time.
pub async fn collect_usage_since(
    tool_id: &str,
    since: DateTime<Utc>,
    home_dir: Option<String>,
) -> Result<UsageSummary> {
    let since_str = since.format("%Y-%m-%d").to_string();

    let options = LocalParseOptions {
        home_dir,
        use_env_roots: true,
        clients: Some(vec![tool_id.to_string()]),
        since: Some(since_str),
        until: None,
        year: None,
    };

    let messages = tokscale_core::parse_local_unified_messages(options).await
        .map_err(|e| anyhow::anyhow!(e))?;

    let since_ts = since.timestamp();
    let filtered: Vec<&UnifiedMessage> = messages.iter()
        .filter(|m| m.timestamp >= since_ts)
        .collect();

    let total_input: i64 = filtered.iter().map(|m| m.tokens.input).sum();
    let total_output: i64 = filtered.iter().map(|m| m.tokens.output).sum();
    let total_cost: f64 = filtered.iter().map(|m| m.cost).sum();
    let message_count: i32 = filtered.iter().map(|m| m.message_count).sum();

    Ok(UsageSummary {
        tool_id: tool_id.to_string(),
        total_input_tokens: total_input,
        total_output_tokens: total_output,
        total_tokens: total_input + total_output,
        total_cost,
        message_count,
        window_start: since,
    })
}

/// Collect usage for the current day.
pub async fn collect_daily_usage(
    tool_id: &str,
    home_dir: Option<String>,
) -> Result<UsageSummary> {
    let today = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    let since = DateTime::<Utc>::from_naive_utc_and_offset(today, Utc);
    collect_usage_since(tool_id, since, home_dir).await
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: Compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add src/data_sources/usage_collector.rs
git commit -m "feat: add usage collector wrapping tokscale-core parsers"
```

---

### Task 6: Claude Quota Provider

**Files:**
- Modify: `src/data_sources/claude_quota.rs`

- [ ] **Step 1: Implement Claude OAuth quota checker**

```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::PathBuf;

use super::{QuotaInfo, QuotaProvider, QuotaWindow};

pub struct ClaudeQuotaProvider {
    credentials_path: PathBuf,
}

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OAuthCreds>,
}

#[derive(Deserialize)]
struct OAuthCreds {
    #[serde(rename = "accessToken")]
    access_token: String,
}

#[derive(Deserialize)]
struct UsageResponse {
    five_hour: Option<UsageWindow>,
    seven_day: Option<UsageWindow>,
}

#[derive(Deserialize)]
struct UsageWindow {
    utilization: f64,
    resets_at: String,
}

impl ClaudeQuotaProvider {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            credentials_path: home.join(".claude").join(".credentials.json"),
        }
    }

    pub fn with_credentials_path(path: PathBuf) -> Self {
        Self { credentials_path: path }
    }

    fn read_access_token(&self) -> Result<String> {
        let content = std::fs::read_to_string(&self.credentials_path)
            .with_context(|| format!(
                "Cannot read Claude credentials at {}. Is Claude Code logged in?",
                self.credentials_path.display()
            ))?;
        let creds: Credentials = serde_json::from_str(&content)
            .context("Failed to parse Claude credentials JSON")?;
        let oauth = creds.claude_ai_oauth
            .context("No claudeAiOauth field in credentials")?;
        Ok(oauth.access_token)
    }
}

#[async_trait]
impl QuotaProvider for ClaudeQuotaProvider {
    fn provider_id(&self) -> &str {
        "claude"
    }

    async fn fetch_quota(&self) -> Result<QuotaInfo> {
        let token = self.read_access_token()?;

        let client = reqwest::Client::new();
        let resp = client
            .get("https://api.anthropic.com/api/oauth/usage")
            .header("Authorization", format!("Bearer {}", token))
            .header("anthropic-beta", "oauth-2025-04-20")
            .send()
            .await
            .context("Failed to call Claude usage API")?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            anyhow::bail!("Claude API rate limited (429). Will retry next cycle.");
        }

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!(
                "Claude OAuth token expired or invalid. Please re-login to Claude Code."
            );
        }

        let usage: UsageResponse = resp.json().await
            .context("Failed to parse Claude usage response")?;

        let mut windows = Vec::new();

        if let Some(w) = usage.five_hour {
            windows.push(QuotaWindow {
                name: "five_hour".to_string(),
                utilization: w.utilization,
                resets_at: w.resets_at.parse::<DateTime<Utc>>().ok(),
                used_tokens: None,
                used_count: None,
            });
        }

        if let Some(w) = usage.seven_day {
            windows.push(QuotaWindow {
                name: "seven_day".to_string(),
                utilization: w.utilization,
                resets_at: w.resets_at.parse::<DateTime<Utc>>().ok(),
                used_tokens: None,
                used_count: None,
            });
        }

        Ok(QuotaInfo {
            provider_id: "claude".to_string(),
            windows,
        })
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: Compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add src/data_sources/claude_quota.rs
git commit -m "feat: add Claude OAuth quota provider"
```

---

### Task 7: Estimated Quota Provider

**Files:**
- Modify: `src/data_sources/estimated_quota.rs`

- [ ] **Step 1: Implement generic estimated quota provider**

```rust
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, Utc};

use super::usage_collector;
use super::{QuotaInfo, QuotaProvider, QuotaWindow};
use crate::config::ProviderConfig;

/// Estimates quota by comparing consumed usage against a user-configured limit.
pub struct EstimatedQuotaProvider {
    tool_id: String,
    config: ProviderConfig,
    home_dir: Option<String>,
}

impl EstimatedQuotaProvider {
    pub fn new(tool_id: String, config: ProviderConfig, home_dir: Option<String>) -> Self {
        Self { tool_id, config, home_dir }
    }

    fn calculate_monthly_reset(&self) -> Option<DateTime<Utc>> {
        let billing_day = self.config.billing_day.unwrap_or(1);
        let now = Utc::now();
        let (year, month) = if now.day() >= billing_day {
            // Reset is next month
            if now.month() == 12 {
                (now.year() + 1, 1)
            } else {
                (now.year(), now.month() + 1)
            }
        } else {
            (now.year(), now.month())
        };

        NaiveDate::from_ymd_opt(year, month, billing_day)
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
    }

    fn calculate_daily_reset() -> DateTime<Utc> {
        let tomorrow = Utc::now().date_naive().succ_opt().unwrap();
        let dt = tomorrow.and_hms_opt(0, 0, 0).unwrap();
        DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
    }

    fn calculate_monthly_window_start(&self) -> DateTime<Utc> {
        let billing_day = self.config.billing_day.unwrap_or(1);
        let now = Utc::now();
        let (year, month) = if now.day() >= billing_day {
            (now.year(), now.month())
        } else {
            if now.month() == 1 {
                (now.year() - 1, 12)
            } else {
                (now.year(), now.month() - 1)
            }
        };

        let d = NaiveDate::from_ymd_opt(year, month, billing_day).unwrap();
        let dt = d.and_hms_opt(0, 0, 0).unwrap();
        DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
    }
}

#[async_trait]
impl QuotaProvider for EstimatedQuotaProvider {
    fn provider_id(&self) -> &str {
        &self.tool_id
    }

    async fn fetch_quota(&self) -> Result<QuotaInfo> {
        let mut windows = Vec::new();

        // Monthly fast requests (Cursor-style)
        if let Some(limit) = self.config.monthly_fast_requests {
            let since = self.calculate_monthly_window_start();
            let usage = usage_collector::collect_usage_since(
                &self.tool_id, since, self.home_dir.clone()
            ).await?;

            let used = usage.message_count as u64;
            let utilization = if limit > 0 { used as f64 / limit as f64 } else { 0.0 };

            windows.push(QuotaWindow {
                name: "monthly".to_string(),
                utilization: utilization.min(1.0),
                resets_at: self.calculate_monthly_reset(),
                used_tokens: None,
                used_count: Some(used),
            });
        }

        // Daily token limit (generic)
        if let Some(limit) = self.config.daily_token_limit {
            let usage = usage_collector::collect_daily_usage(
                &self.tool_id, self.home_dir.clone()
            ).await?;

            let used = usage.total_tokens;
            let utilization = if limit > 0 { used as f64 / limit as f64 } else { 0.0 };

            windows.push(QuotaWindow {
                name: "daily".to_string(),
                utilization: utilization.min(1.0),
                resets_at: Some(Self::calculate_daily_reset()),
                used_tokens: Some(used),
                used_count: None,
            });
        }

        Ok(QuotaInfo {
            provider_id: self.tool_id.clone(),
            windows,
        })
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: Compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add src/data_sources/estimated_quota.rs
git commit -m "feat: add generic estimated quota provider for any tool"
```

---

### Task 8: Threshold Engine

**Files:**
- Create: `src/threshold.rs`
- Create: `tests/threshold_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/threshold_test.rs`:

```rust
use psst::threshold::{evaluate_thresholds, AlertEvent, AlertKind};
use psst::state::QuotaWindowState;
use chrono::{Duration, Utc};

#[test]
fn test_no_alerts_below_threshold() {
    let window = QuotaWindowState {
        utilization: 0.30,
        resets_at: Some((Utc::now() + Duration::hours(48)).to_rfc3339()),
        alerts_sent: vec![],
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds(
        "claude", "five_hour", &window,
        &[50, 80], &[24, 12, 1], 0.95,
    );
    assert!(alerts.is_empty());
}

#[test]
fn test_usage_alert_at_50_percent() {
    let window = QuotaWindowState {
        utilization: 0.55,
        resets_at: Some((Utc::now() + Duration::hours(48)).to_rfc3339()),
        alerts_sent: vec![],
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds(
        "claude", "five_hour", &window,
        &[50, 80], &[24, 12, 1], 0.95,
    );
    assert_eq!(alerts.len(), 1);
    assert!(matches!(alerts[0].kind, AlertKind::UsageThreshold(50)));
}

#[test]
fn test_no_duplicate_usage_alert() {
    let window = QuotaWindowState {
        utilization: 0.55,
        resets_at: Some((Utc::now() + Duration::hours(48)).to_rfc3339()),
        alerts_sent: vec![50],  // already sent
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds(
        "claude", "five_hour", &window,
        &[50, 80], &[24, 12, 1], 0.95,
    );
    assert!(alerts.is_empty());
}

#[test]
fn test_multiple_thresholds_crossed() {
    let window = QuotaWindowState {
        utilization: 0.85,
        resets_at: Some((Utc::now() + Duration::hours(48)).to_rfc3339()),
        alerts_sent: vec![],
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds(
        "claude", "five_hour", &window,
        &[50, 80], &[24, 12, 1], 0.95,
    );
    assert_eq!(alerts.len(), 2);
}

#[test]
fn test_reset_countdown_alert() {
    let window = QuotaWindowState {
        utilization: 0.30,
        resets_at: Some((Utc::now() + Duration::minutes(30)).to_rfc3339()),
        alerts_sent: vec![],
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds(
        "claude", "seven_day", &window,
        &[50, 80], &[24, 12, 1], 0.95,
    );
    // Should trigger 24h, 12h, and 1h alerts (all are <= remaining)
    assert_eq!(alerts.len(), 3);
    assert!(alerts.iter().all(|a| matches!(a.kind, AlertKind::ResetCountdown(_))));
}

#[test]
fn test_no_reset_alert_when_usage_above_skip_threshold() {
    let window = QuotaWindowState {
        utilization: 0.96,  // above 0.95 skip threshold
        resets_at: Some((Utc::now() + Duration::minutes(30)).to_rfc3339()),
        alerts_sent: vec![],
        reset_alerts_sent: vec![],
        ..Default::default()
    };
    let alerts = evaluate_thresholds(
        "claude", "seven_day", &window,
        &[50, 80], &[24, 12, 1], 0.95,
    );
    // Usage alerts should fire (50, 80) but no reset alerts
    let reset_alerts: Vec<_> = alerts.iter()
        .filter(|a| matches!(a.kind, AlertKind::ResetCountdown(_)))
        .collect();
    assert!(reset_alerts.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test threshold_test 2>&1 | tail -5`
Expected: FAIL — module `threshold` not found.

- [ ] **Step 3: Implement threshold.rs**

Create `src/threshold.rs`:

```rust
use chrono::{DateTime, Utc};
use crate::state::QuotaWindowState;

#[derive(Debug, Clone)]
pub enum AlertKind {
    /// Usage crossed a percentage threshold (e.g. 50, 80)
    UsageThreshold(u32),
    /// Reset countdown in hours (e.g. 24, 12, 1)
    ResetCountdown(u32),
}

#[derive(Debug, Clone)]
pub struct AlertEvent {
    pub provider_id: String,
    pub window_name: String,
    pub kind: AlertKind,
    pub utilization: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

/// Evaluate threshold rules against a quota window state.
/// Returns a list of alert events that should be dispatched.
pub fn evaluate_thresholds(
    provider_id: &str,
    window_name: &str,
    window: &QuotaWindowState,
    usage_alerts: &[u32],
    reset_alerts_hours: &[u32],
    skip_reset_alert_above: f64,
) -> Vec<AlertEvent> {
    let mut events = Vec::new();
    let utilization_pct = (window.utilization * 100.0) as u32;

    let resets_at: Option<DateTime<Utc>> = window.resets_at.as_ref()
        .and_then(|s| s.parse::<DateTime<Utc>>().ok());

    // Rule A: Usage thresholds
    for &threshold in usage_alerts {
        if utilization_pct >= threshold && !window.alerts_sent.contains(&threshold) {
            events.push(AlertEvent {
                provider_id: provider_id.to_string(),
                window_name: window_name.to_string(),
                kind: AlertKind::UsageThreshold(threshold),
                utilization: window.utilization,
                resets_at,
            });
        }
    }

    // Rule B: Reset countdown (only if utilization is below skip threshold)
    if window.utilization < skip_reset_alert_above {
        if let Some(reset_time) = resets_at {
            let now = Utc::now();
            if reset_time > now {
                let remaining_hours = (reset_time - now).num_hours() as u32;
                // Also check fractional hours (e.g. 30 minutes remaining should trigger 1h alert)
                let remaining_minutes = (reset_time - now).num_minutes() as u32;

                for &hours in reset_alerts_hours {
                    let threshold_minutes = hours * 60;
                    if remaining_minutes <= threshold_minutes
                        && !window.reset_alerts_sent.contains(&hours)
                    {
                        events.push(AlertEvent {
                            provider_id: provider_id.to_string(),
                            window_name: window_name.to_string(),
                            kind: AlertKind::ResetCountdown(hours),
                            utilization: window.utilization,
                            resets_at: Some(reset_time),
                        });
                    }
                }
            }
        }
    }

    events
}

/// After dispatching alerts, record them in state so they don't fire again.
pub fn record_alerts(window: &mut QuotaWindowState, events: &[AlertEvent]) {
    for event in events {
        match &event.kind {
            AlertKind::UsageThreshold(pct) => {
                if !window.alerts_sent.contains(pct) {
                    window.alerts_sent.push(*pct);
                }
            }
            AlertKind::ResetCountdown(hours) => {
                if !window.reset_alerts_sent.contains(hours) {
                    window.reset_alerts_sent.push(*hours);
                }
            }
        }
    }
}
```

Update `src/main.rs`:

```rust
pub mod config;
pub mod data_sources;
pub mod state;
pub mod threshold;

fn main() {
    println!("Psst - AI coding tool usage monitor");
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test threshold_test -- --nocapture 2>&1 | tail -15`
Expected: All 6 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/threshold.rs tests/threshold_test.rs src/main.rs
git commit -m "feat: add threshold engine with usage and reset countdown rules"
```

---

### Task 9: Notifier Trait & Dispatcher

**Files:**
- Create: `src/notifiers/mod.rs`

- [ ] **Step 1: Implement Notifier trait and Dispatcher**

Create `src/notifiers/mod.rs`:

```rust
pub mod desktop;
pub mod telegram;
pub mod serverchan;
pub mod web_push_notifier;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::threshold::{AlertEvent, AlertKind};

/// A formatted notification ready to send
#[derive(Debug, Clone)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub provider_id: String,
    pub window_name: String,
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, notification: &Notification) -> Result<()>;
    fn name(&self) -> &str;
    fn is_enabled(&self) -> bool;
}

/// Dispatches notifications to all enabled channels
pub struct Dispatcher {
    notifiers: Vec<Box<dyn Notifier>>,
}

impl Dispatcher {
    pub fn new(notifiers: Vec<Box<dyn Notifier>>) -> Self {
        Self { notifiers }
    }

    pub async fn dispatch(&self, notification: &Notification) {
        for notifier in &self.notifiers {
            if !notifier.is_enabled() {
                continue;
            }
            if let Err(e) = notifier.send(notification).await {
                tracing::warn!(
                    "Failed to send via {}: {}",
                    notifier.name(),
                    e
                );
            }
        }
    }
}

/// Format an AlertEvent into a Notification
pub fn format_notification(event: &AlertEvent) -> Notification {
    let window_display = format_window_name(&event.window_name);
    let provider_display = capitalize(&event.provider_id);
    let utilization_pct = (event.utilization * 100.0) as u32;
    let remaining_pct = 100 - utilization_pct.min(100);

    let reset_info = event.resets_at.map(|r| {
        let now = Utc::now();
        if r > now {
            let dur = r - now;
            let hours = dur.num_hours();
            let minutes = dur.num_minutes() % 60;
            if hours > 24 {
                format!("{}天后", hours / 24)
            } else if hours > 0 {
                format!("{}小时{}分钟后", hours, minutes)
            } else {
                format!("{}分钟后", dur.num_minutes())
            }
        } else {
            "已重置".to_string()
        }
    });

    match &event.kind {
        AlertKind::UsageThreshold(pct) => {
            let title = format!(
                "Psst! {} {}已用 {}%",
                provider_display, window_display, pct
            );
            let mut body = format!(
                "当前用量：{}%\n剩余额度：约 {}%",
                utilization_pct, remaining_pct
            );
            if let Some(reset) = reset_info {
                body.push_str(&format!("\n重置时间：{}", reset));
            }
            Notification {
                title,
                body,
                provider_id: event.provider_id.clone(),
                window_name: event.window_name.clone(),
            }
        }
        AlertKind::ResetCountdown(hours) => {
            let time_str = if *hours >= 24 {
                format!("{}天", hours / 24)
            } else {
                format!("{}小时", hours)
            };
            let title = format!(
                "Psst! {} {} {}后重置",
                provider_display, window_display, time_str
            );
            let mut body = format!(
                "当前用量：仅 {}%\n剩余额度：约 {}% 未使用",
                utilization_pct, remaining_pct
            );
            if remaining_pct > 10 {
                body.push_str("\n建议在重置前充分利用剩余额度");
            }
            if let Some(reset) = reset_info {
                body.push_str(&format!("\n重置时间：{}", reset));
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

fn format_window_name(name: &str) -> String {
    match name {
        "five_hour" => "5小时窗口".to_string(),
        "seven_day" => "7天窗口".to_string(),
        "monthly" => "月度配额".to_string(),
        "daily" => "日配额".to_string(),
        other => other.to_string(),
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}
```

- [ ] **Step 2: Create stubs for notifier submodules**

Create `src/notifiers/desktop.rs`:
```rust
// Desktop notifier — implemented in Task 10
```

Create `src/notifiers/telegram.rs`:
```rust
// Telegram notifier — implemented in Task 11
```

Create `src/notifiers/serverchan.rs`:
```rust
// ServerChan notifier — implemented in Task 12
```

Create `src/notifiers/web_push_notifier.rs`:
```rust
// Web Push notifier — implemented in Task 16
```

Update `src/main.rs`:

```rust
pub mod config;
pub mod data_sources;
pub mod notifiers;
pub mod state;
pub mod threshold;

fn main() {
    println!("Psst - AI coding tool usage monitor");
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: Compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add src/notifiers/ src/main.rs
git commit -m "feat: add Notifier trait, Dispatcher, and notification formatting"
```

---

### Task 10: Desktop Notifier

**Files:**
- Modify: `src/notifiers/desktop.rs`

- [ ] **Step 1: Implement desktop notifier**

```rust
use anyhow::Result;
use async_trait::async_trait;
use notify_rust::Notification as DesktopNotification;

use super::{Notification, Notifier};

pub struct DesktopNotifier {
    enabled: bool,
}

impl DesktopNotifier {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
}

#[async_trait]
impl Notifier for DesktopNotifier {
    async fn send(&self, notification: &Notification) -> Result<()> {
        DesktopNotification::new()
            .summary(&notification.title)
            .body(&notification.body)
            .appname("Psst")
            .show()?;
        Ok(())
    }

    fn name(&self) -> &str {
        "desktop"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add src/notifiers/desktop.rs
git commit -m "feat: add macOS desktop notifier via notify-rust"
```

---

### Task 11: Telegram Notifier

**Files:**
- Modify: `src/notifiers/telegram.rs`

- [ ] **Step 1: Implement Telegram Bot API notifier**

```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Serialize;

use super::{Notification, Notifier};

pub struct TelegramNotifier {
    enabled: bool,
    bot_token: String,
    chat_id: String,
    client: reqwest::Client,
}

#[derive(Serialize)]
struct SendMessage {
    chat_id: String,
    text: String,
    parse_mode: String,
}

impl TelegramNotifier {
    pub fn new(enabled: bool, bot_token: String, chat_id: String) -> Self {
        Self {
            enabled,
            bot_token,
            chat_id,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Notifier for TelegramNotifier {
    async fn send(&self, notification: &Notification) -> Result<()> {
        let text = format!("*{}*\n\n{}", notification.title, notification.body);

        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        let payload = SendMessage {
            chat_id: self.chat_id.clone(),
            text,
            parse_mode: "Markdown".to_string(),
        };

        let resp = self.client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Failed to send Telegram message")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Telegram API error {}: {}", status, body);
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "telegram"
    }

    fn is_enabled(&self) -> bool {
        self.enabled && !self.bot_token.is_empty() && !self.chat_id.is_empty()
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add src/notifiers/telegram.rs
git commit -m "feat: add Telegram Bot API notifier"
```

---

### Task 12: ServerChan Notifier

**Files:**
- Modify: `src/notifiers/serverchan.rs`

- [ ] **Step 1: Implement Server酱 notifier**

```rust
use anyhow::{Context, Result};
use async_trait::async_trait;

use super::{Notification, Notifier};

pub struct ServerChanNotifier {
    enabled: bool,
    send_key: String,
    client: reqwest::Client,
}

impl ServerChanNotifier {
    pub fn new(enabled: bool, send_key: String) -> Self {
        Self {
            enabled,
            send_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Notifier for ServerChanNotifier {
    async fn send(&self, notification: &Notification) -> Result<()> {
        let url = format!("https://sctapi.ftqq.com/{}.send", self.send_key);

        let params = [
            ("title", &notification.title),
            ("desp", &notification.body),
        ];

        let resp = self.client
            .post(&url)
            .form(&params)
            .send()
            .await
            .context("Failed to send ServerChan message")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("ServerChan API error {}: {}", status, body);
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "serverchan"
    }

    fn is_enabled(&self) -> bool {
        self.enabled && !self.send_key.is_empty()
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add src/notifiers/serverchan.rs
git commit -m "feat: add ServerChan (WeChat) notifier"
```

---

### Task 13: Scheduler (Core Loop)

**Files:**
- Create: `src/scheduler.rs`

- [ ] **Step 1: Implement the scheduler**

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

use crate::config::Config;
use crate::data_sources::claude_quota::ClaudeQuotaProvider;
use crate::data_sources::estimated_quota::EstimatedQuotaProvider;
use crate::data_sources::discovery::discover_tools;
use crate::data_sources::QuotaProvider;
use crate::notifiers::{format_notification, Dispatcher};
use crate::state::{AppState, ProviderState, QuotaWindowState};
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
        state: AppState,
        dispatcher: Dispatcher,
    ) -> Self {
        let home_dir = dirs::home_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        Self {
            config,
            state_path,
            state: Arc::new(Mutex::new(state)),
            dispatcher: Arc::new(dispatcher),
            home_dir,
        }
    }

    /// Run the main scheduler loop
    pub async fn run(&self) {
        tracing::info!("Psst scheduler started (interval: {}min)", self.config.general.check_interval_minutes);

        // Run immediately on start
        self.check_once().await;

        let mut ticker = interval(Duration::from_secs(
            self.config.general.check_interval_minutes as u64 * 60
        ));
        ticker.tick().await; // skip first immediate tick

        loop {
            ticker.tick().await;
            self.check_once().await;
        }
    }

    /// Single check cycle
    pub async fn check_once(&self) {
        tracing::info!("Running usage check...");

        let mut state = self.state.lock().await;

        // Step 1: Clear expired windows
        state.clear_expired_windows();

        // Step 2: Auto-discover tools
        if self.config.general.auto_discover {
            let discovered = discover_tools(&self.home_dir);
            if discovered != state.discovered_tools {
                tracing::info!("Discovered tools: {:?}", discovered);
                state.discovered_tools = discovered;
            }
        }

        // Step 3: Build quota providers
        let providers = self.build_providers();

        // Step 4: Fetch quotas and evaluate thresholds
        for provider in &providers {
            let pid = provider.provider_id().to_string();

            match provider.fetch_quota().await {
                Ok(quota_info) => {
                    let provider_state = state.providers
                        .entry(pid.clone())
                        .or_insert_with(ProviderState::default);

                    for window in &quota_info.windows {
                        let window_state = provider_state.windows
                            .entry(window.name.clone())
                            .or_insert_with(QuotaWindowState::default);

                        // Update state with latest data
                        window_state.utilization = window.utilization;
                        window_state.resets_at = window.resets_at.map(|r| r.to_rfc3339());
                        if let Some(t) = window.used_tokens {
                            window_state.used_tokens = Some(t);
                        }
                        if let Some(c) = window.used_count {
                            window_state.used_count = Some(c);
                        }

                        // Evaluate thresholds
                        let alerts = evaluate_thresholds(
                            &pid,
                            &window.name,
                            window_state,
                            &self.config.thresholds.usage_alerts,
                            &self.config.thresholds.reset_alerts_hours,
                            self.config.thresholds.skip_reset_alert_above,
                        );

                        // Dispatch notifications
                        for alert in &alerts {
                            let notification = format_notification(alert);
                            tracing::info!("Alert: {}", notification.title);
                            self.dispatcher.dispatch(&notification).await;
                        }

                        // Record sent alerts
                        record_alerts(window_state, &alerts);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch quota for {}: {}", pid, e);
                }
            }
        }

        // Step 5: Save state
        state.mark_checked();
        if let Err(e) = state.save_atomic(&self.state_path) {
            tracing::error!("Failed to save state: {}", e);
        }
    }

    fn build_providers(&self) -> Vec<Box<dyn QuotaProvider>> {
        let mut providers: Vec<Box<dyn QuotaProvider>> = Vec::new();

        for (name, provider_config) in &self.config.providers {
            match name.as_str() {
                "claude" => {
                    providers.push(Box::new(ClaudeQuotaProvider::new()));
                }
                _ => {
                    // Only add estimated provider if it has a configured limit
                    if provider_config.monthly_fast_requests.is_some()
                        || provider_config.daily_token_limit.is_some()
                    {
                        providers.push(Box::new(EstimatedQuotaProvider::new(
                            name.clone(),
                            provider_config.clone(),
                            Some(self.home_dir.clone()),
                        )));
                    }
                }
            }
        }

        providers
    }

    /// Get current state (for API/status command)
    pub async fn get_state(&self) -> AppState {
        self.state.lock().await.clone()
    }
}
```

Update `src/main.rs`:

```rust
pub mod config;
pub mod data_sources;
pub mod notifiers;
pub mod scheduler;
pub mod state;
pub mod threshold;

fn main() {
    println!("Psst - AI coding tool usage monitor");
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add src/scheduler.rs src/main.rs
git commit -m "feat: add scheduler with check loop, quota fetch, and alert dispatch"
```

---

### Task 14: CLI (main.rs with clap)

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Implement CLI with clap commands**

Replace `src/main.rs`:

```rust
pub mod config;
pub mod data_sources;
pub mod notifiers;
pub mod scheduler;
pub mod state;
pub mod threshold;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use config::Config;
use notifiers::desktop::DesktopNotifier;
use notifiers::telegram::TelegramNotifier;
use notifiers::serverchan::ServerChanNotifier;
use notifiers::Dispatcher;
use scheduler::Scheduler;
use state::AppState;

#[derive(Parser)]
#[command(name = "psst", about = "AI coding tool usage monitor & notifier")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize config and VAPID keys (first-time setup)
    Init,
    /// Run the daemon (foreground)
    Run,
    /// Show current usage status
    Status,
    /// Install as macOS LaunchAgent
    Install,
    /// Uninstall LaunchAgent
    Uninstall,
}

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("psst")
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

fn state_path() -> PathBuf {
    config_dir().join("state.json")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => cmd_init()?,
        Commands::Run => cmd_run().await?,
        Commands::Status => cmd_status()?,
        Commands::Install => cmd_install()?,
        Commands::Uninstall => cmd_uninstall()?,
    }

    Ok(())
}

fn cmd_init() -> Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;

    let cfg_path = config_path();
    if cfg_path.exists() {
        println!("Config already exists at {}", cfg_path.display());
    } else {
        std::fs::write(&cfg_path, Config::default_config_toml())?;
        println!("Created config at {}", cfg_path.display());
    }

    let st_path = state_path();
    if !st_path.exists() {
        let mut state = AppState::default();
        state.ensure_access_token();
        state.save_atomic(&st_path)?;
        println!("Created state at {}", st_path.display());
        println!("Web access token: {}", state.access_token.unwrap_or_default());
    }

    println!("\nEdit {} to configure notifications.", cfg_path.display());
    println!("Then run: psst run");
    Ok(())
}

async fn cmd_run() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("psst=info".parse().unwrap()),
        )
        .init();

    let cfg_path = config_path();
    let config = if cfg_path.exists() {
        Config::load_from(&cfg_path)?
    } else {
        tracing::warn!("No config found, using defaults. Run 'psst init' first.");
        Config::default()
    };

    let st_path = state_path();
    let mut state = AppState::load_or_default(&st_path);
    state.ensure_access_token();

    tracing::info!("Psst starting...");
    tracing::info!("Config: {}", cfg_path.display());
    tracing::info!("State: {}", st_path.display());

    // Build notifiers
    let notifiers: Vec<Box<dyn notifiers::Notifier>> = vec![
        Box::new(DesktopNotifier::new(config.notifications.desktop)),
        Box::new(TelegramNotifier::new(
            config.notifications.telegram.enabled,
            config.notifications.telegram.bot_token.clone(),
            config.notifications.telegram.chat_id.clone(),
        )),
        Box::new(ServerChanNotifier::new(
            config.notifications.serverchan.enabled,
            config.notifications.serverchan.send_key.clone(),
        )),
    ];

    let dispatcher = Dispatcher::new(notifiers);
    let scheduler = Scheduler::new(config, st_path, state, dispatcher);

    scheduler.run().await;

    Ok(())
}

fn cmd_status() -> Result<()> {
    let st_path = state_path();
    let state = AppState::load_or_default(&st_path);

    println!("Psst Status");
    println!("===========\n");

    if let Some(ref last) = state.last_check_at {
        println!("Last check: {}", last);
    } else {
        println!("Last check: never");
    }

    println!("Discovered tools: {:?}\n", state.discovered_tools);

    for (name, provider) in &state.providers {
        println!("{}:", name);
        for (window_name, window) in &provider.windows {
            let pct = (window.utilization * 100.0) as u32;
            let remaining = 100 - pct.min(100);
            print!("  {}: {}% used, {}% remaining", window_name, pct, remaining);
            if let Some(ref r) = window.resets_at {
                print!(" (resets: {})", r);
            }
            println!();
        }
    }

    Ok(())
}

fn cmd_install() -> Result<()> {
    let plist_dir = dirs::home_dir()
        .unwrap_or_default()
        .join("Library/LaunchAgents");
    std::fs::create_dir_all(&plist_dir)?;

    let plist_path = plist_dir.join("com.psst.notify.plist");
    let exe = std::env::current_exe()?.to_string_lossy().to_string();

    let plist_content = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.psst.notify</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>run</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/tmp/psst.out.log</string>
  <key>StandardErrorPath</key>
  <string>/tmp/psst.err.log</string>
</dict>
</plist>"#, exe);

    std::fs::write(&plist_path, plist_content)?;

    println!("Installed LaunchAgent at {}", plist_path.display());
    println!("Loading...");

    std::process::Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .status()?;

    println!("Psst is now running in background.");
    Ok(())
}

fn cmd_uninstall() -> Result<()> {
    let plist_path = dirs::home_dir()
        .unwrap_or_default()
        .join("Library/LaunchAgents/com.psst.notify.plist");

    if plist_path.exists() {
        std::process::Command::new("launchctl")
            .args(["unload", &plist_path.to_string_lossy()])
            .status()?;
        std::fs::remove_file(&plist_path)?;
        println!("Uninstalled LaunchAgent.");
    } else {
        println!("LaunchAgent not found.");
    }

    Ok(())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: Compiles.

- [ ] **Step 3: Test CLI help**

Run: `cargo run -- --help 2>&1`
Expected: Shows help with subcommands: init, run, status, install, uninstall.

- [ ] **Step 4: Test init command**

Run: `cargo run -- init 2>&1`
Expected: Creates `~/.config/psst/config.toml` and `state.json`, prints access token.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: add CLI with init, run, status, install, uninstall commands"
```

---

### Task 15: Web Server + PWA Dashboard

**Files:**
- Create: `src/web/mod.rs`
- Create: `src/web/api.rs`
- Create: `src/web/static/index.html`
- Create: `src/web/static/manifest.json`
- Create: `src/web/static/sw.js`
- Create: `src/web/static/app.js`

- [ ] **Step 1: Implement axum web server**

Create `src/web/mod.rs`:

```rust
pub mod api;

use axum::{
    Router,
    routing::get,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

use crate::config::Config;
use crate::state::AppState;

pub struct WebServer {
    bind: String,
    state: Arc<Mutex<AppState>>,
    access_token: Option<String>,
}

impl WebServer {
    pub fn new(bind: String, state: Arc<Mutex<AppState>>, access_token: Option<String>) -> Self {
        Self { bind, state, access_token }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let shared_state = self.state.clone();
        let access_token = self.access_token.clone();

        let app = Router::new()
            .route("/", get(api::index_handler))
            .route("/manifest.json", get(api::manifest_handler))
            .route("/sw.js", get(api::sw_handler))
            .route("/app.js", get(api::app_handler))
            .route("/api/status", get(api::status_handler))
            .route("/api/health", get(api::health_handler))
            .route("/api/subscribe", axum::routing::post(api::subscribe_handler))
            .layer(CorsLayer::permissive())
            .with_state(api::AppContext {
                state: shared_state,
                access_token,
            });

        let addr: SocketAddr = self.bind.parse()?;
        tracing::info!("Web server listening on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}
```

Create `src/web/api.rs`:

```rust
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::state::{AppState, PushKeys, PushSubscription};

#[derive(Clone)]
pub struct AppContext {
    pub state: Arc<Mutex<AppState>>,
    pub access_token: Option<String>,
}

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

fn check_token(ctx: &AppContext, query: &TokenQuery) -> bool {
    match &ctx.access_token {
        None => true,
        Some(expected) => query.token.as_deref() == Some(expected),
    }
}

pub async fn index_handler(
    State(ctx): State<AppContext>,
    Query(query): Query<TokenQuery>,
) -> impl IntoResponse {
    if !check_token(&ctx, &query) {
        return (StatusCode::UNAUTHORIZED, Html("Unauthorized".to_string()));
    }
    (StatusCode::OK, Html(include_str!("static/index.html").to_string()))
}

pub async fn manifest_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        include_str!("static/manifest.json"),
    )
}

pub async fn sw_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("static/sw.js"),
    )
}

pub async fn app_handler() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("static/app.js"),
    )
}

pub async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

pub async fn status_handler(
    State(ctx): State<AppContext>,
    Query(query): Query<TokenQuery>,
) -> impl IntoResponse {
    if !check_token(&ctx, &query) {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "unauthorized"}))).into_response();
    }
    let state = ctx.state.lock().await;
    Json(serde_json::json!({
        "last_check_at": state.last_check_at,
        "discovered_tools": state.discovered_tools,
        "providers": state.providers,
    })).into_response()
}

#[derive(Deserialize)]
pub struct SubscribeRequest {
    pub endpoint: String,
    pub keys: SubscribeKeys,
}

#[derive(Deserialize)]
pub struct SubscribeKeys {
    pub p256dh: String,
    pub auth: String,
}

pub async fn subscribe_handler(
    State(ctx): State<AppContext>,
    Json(payload): Json<SubscribeRequest>,
) -> impl IntoResponse {
    let mut state = ctx.state.lock().await;

    // Deduplicate by endpoint
    state.push_subscriptions.retain(|s| s.endpoint != payload.endpoint);

    state.push_subscriptions.push(PushSubscription {
        endpoint: payload.endpoint,
        keys: PushKeys {
            p256dh: payload.keys.p256dh,
            auth: payload.keys.auth,
        },
        created_at: chrono::Utc::now().to_rfc3339(),
    });

    tracing::info!("New push subscription registered ({} total)", state.push_subscriptions.len());

    Json(serde_json::json!({ "status": "subscribed" }))
}
```

- [ ] **Step 2: Create PWA static files**

Create `src/web/static/manifest.json`:

```json
{
  "name": "Psst",
  "short_name": "Psst",
  "description": "AI coding tool usage monitor",
  "start_url": "/",
  "display": "standalone",
  "background_color": "#1a1a2e",
  "theme_color": "#e94560"
}
```

Create `src/web/static/sw.js`:

```javascript
self.addEventListener('push', (event) => {
  const data = event.data ? event.data.json() : {};
  const title = data.title || 'Psst!';
  const options = {
    body: data.body || '',
    icon: data.icon || undefined,
    badge: data.badge || undefined,
    tag: data.tag || 'psst-notification',
  };
  event.waitUntil(self.registration.showNotification(title, options));
});

self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  event.waitUntil(clients.openWindow('/'));
});
```

Create `src/web/static/app.js`:

```javascript
async function fetchStatus() {
  try {
    const params = new URLSearchParams(window.location.search);
    const token = params.get('token') || '';
    const resp = await fetch(`/api/status?token=${token}`);
    if (!resp.ok) return null;
    return await resp.json();
  } catch (e) {
    console.error('Failed to fetch status:', e);
    return null;
  }
}

function renderStatus(data) {
  const container = document.getElementById('status');
  if (!data) {
    container.innerHTML = '<p class="error">Failed to load status</p>';
    return;
  }

  let html = '';
  if (data.last_check_at) {
    html += `<p class="meta">Last check: ${new Date(data.last_check_at).toLocaleString()}</p>`;
  }
  if (data.discovered_tools && data.discovered_tools.length > 0) {
    html += `<p class="meta">Tools: ${data.discovered_tools.join(', ')}</p>`;
  }

  if (data.providers) {
    for (const [name, provider] of Object.entries(data.providers)) {
      html += `<div class="provider"><h2>${name}</h2>`;
      if (provider.windows) {
        for (const [wName, w] of Object.entries(provider.windows)) {
          const pct = Math.round((w.utilization || 0) * 100);
          const remaining = 100 - pct;
          const barColor = pct >= 80 ? '#e94560' : pct >= 50 ? '#f5a623' : '#0f3460';
          html += `
            <div class="window">
              <h3>${wName}</h3>
              <div class="bar-bg"><div class="bar-fill" style="width:${pct}%;background:${barColor}"></div></div>
              <p>${pct}% used / ${remaining}% remaining</p>
              ${w.resets_at ? `<p class="reset">Resets: ${new Date(w.resets_at).toLocaleString()}</p>` : ''}
            </div>`;
        }
      }
      html += '</div>';
    }
  }

  container.innerHTML = html || '<p>No data yet. Waiting for first check...</p>';
}

async function subscribePush() {
  if (!('serviceWorker' in navigator) || !('PushManager' in window)) {
    document.getElementById('push-status').textContent = 'Push not supported in this browser';
    return;
  }

  try {
    const reg = await navigator.serviceWorker.register('/sw.js');
    const permission = await Notification.requestPermission();
    if (permission !== 'granted') {
      document.getElementById('push-status').textContent = 'Notification permission denied';
      return;
    }

    // Get VAPID public key from server (we'll add this endpoint later)
    // For now, subscription is handled via the subscribe endpoint
    document.getElementById('push-status').textContent = 'Push notifications enabled';
  } catch (e) {
    document.getElementById('push-status').textContent = `Error: ${e.message}`;
  }
}

// Auto-refresh every 60 seconds
async function init() {
  const data = await fetchStatus();
  renderStatus(data);
  setInterval(async () => {
    const data = await fetchStatus();
    renderStatus(data);
  }, 60000);
}

init();
```

Create `src/web/static/index.html`:

```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Psst - AI Usage Monitor</title>
  <link rel="manifest" href="/manifest.json">
  <meta name="theme-color" content="#e94560">
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
      background: #1a1a2e;
      color: #eee;
      padding: 20px;
      max-width: 600px;
      margin: 0 auto;
    }
    h1 { color: #e94560; margin-bottom: 8px; }
    .subtitle { color: #888; margin-bottom: 24px; font-size: 14px; }
    .meta { color: #888; font-size: 13px; margin-bottom: 8px; }
    .provider {
      background: #16213e;
      border-radius: 12px;
      padding: 16px;
      margin-bottom: 16px;
    }
    .provider h2 {
      color: #e94560;
      font-size: 18px;
      margin-bottom: 12px;
      text-transform: capitalize;
    }
    .window { margin-bottom: 12px; }
    .window h3 { font-size: 14px; color: #aaa; margin-bottom: 6px; }
    .bar-bg {
      background: #0f3460;
      border-radius: 8px;
      height: 24px;
      overflow: hidden;
      margin-bottom: 6px;
    }
    .bar-fill {
      height: 100%;
      border-radius: 8px;
      transition: width 0.5s ease;
    }
    .window p { font-size: 13px; color: #ccc; }
    .reset { color: #888; }
    .error { color: #e94560; }
    .push-section {
      margin-top: 24px;
      padding: 16px;
      background: #16213e;
      border-radius: 12px;
    }
    .push-btn {
      background: #e94560;
      border: none;
      color: white;
      padding: 10px 20px;
      border-radius: 8px;
      font-size: 14px;
      cursor: pointer;
    }
    .push-btn:hover { background: #c73e54; }
    #push-status { margin-top: 8px; font-size: 13px; color: #888; }
  </style>
</head>
<body>
  <h1>Psst</h1>
  <p class="subtitle">AI coding tool usage monitor</p>
  <div id="status"><p>Loading...</p></div>
  <div class="push-section">
    <button class="push-btn" onclick="subscribePush()">Enable Push Notifications</button>
    <p id="push-status"></p>
  </div>
  <script src="/app.js"></script>
</body>
</html>
```

- [ ] **Step 3: Update main.rs to start web server alongside scheduler**

Add to the `cmd_run` function in `src/main.rs`, after building the scheduler and before `scheduler.run()`:

Replace the end of `cmd_run` with:

```rust
    // Share state between scheduler and web server
    let shared_state = scheduler.shared_state();
    let access_token = {
        let s = shared_state.lock().await;
        s.access_token.clone()
    };

    // Start web server in background
    let web_bind = config_clone.server.bind.clone();
    let web_state = shared_state.clone();
    tokio::spawn(async move {
        let server = crate::web::WebServer::new(web_bind, web_state, access_token);
        if let Err(e) = server.run().await {
            tracing::error!("Web server error: {}", e);
        }
    });

    // Run scheduler (blocks)
    scheduler.run().await;
```

This requires exposing `shared_state()` from Scheduler. Add to `src/scheduler.rs`:

```rust
    pub fn shared_state(&self) -> Arc<Mutex<AppState>> {
        self.state.clone()
    }
```

And update `cmd_run` in `src/main.rs` fully:

```rust
async fn cmd_run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("psst=info".parse().unwrap()),
        )
        .init();

    let cfg_path = config_path();
    let config = if cfg_path.exists() {
        Config::load_from(&cfg_path)?
    } else {
        tracing::warn!("No config found, using defaults. Run 'psst init' first.");
        Config::default()
    };

    let st_path = state_path();
    let mut state = AppState::load_or_default(&st_path);
    state.ensure_access_token();

    tracing::info!("Psst starting...");
    tracing::info!("Config: {}", cfg_path.display());
    tracing::info!("State: {}", st_path.display());

    let notifiers_list: Vec<Box<dyn notifiers::Notifier>> = vec![
        Box::new(DesktopNotifier::new(config.notifications.desktop)),
        Box::new(TelegramNotifier::new(
            config.notifications.telegram.enabled,
            config.notifications.telegram.bot_token.clone(),
            config.notifications.telegram.chat_id.clone(),
        )),
        Box::new(ServerChanNotifier::new(
            config.notifications.serverchan.enabled,
            config.notifications.serverchan.send_key.clone(),
        )),
    ];

    let dispatcher = Dispatcher::new(notifiers_list);
    let web_bind = config.server.bind.clone();
    let scheduler = Scheduler::new(config, st_path, state, dispatcher);

    // Start web server in background
    let shared_state = scheduler.shared_state();
    let access_token = {
        let s = shared_state.lock().await;
        s.access_token.clone()
    };
    let web_state = shared_state.clone();
    tokio::spawn(async move {
        let server = web::WebServer::new(web_bind, web_state, access_token);
        if let Err(e) = server.run().await {
            tracing::error!("Web server error: {}", e);
        }
    });

    // Run scheduler (blocks)
    scheduler.run().await;

    Ok(())
}
```

Add `pub mod web;` to `src/main.rs` module declarations.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check 2>&1 | tail -10`
Expected: Compiles with no errors.

- [ ] **Step 5: Commit**

```bash
git add src/web/ src/main.rs src/scheduler.rs
git commit -m "feat: add axum web server with PWA dashboard and push subscription"
```

---

### Task 16: Web Push Notifier

**Files:**
- Modify: `src/notifiers/web_push_notifier.rs`

- [ ] **Step 1: Implement Web Push notifier**

```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use web_push::{
    ContentEncoding, SubscriptionInfo, VapidSignatureBuilder,
    WebPushClient, WebPushMessageBuilder, IsahcWebPushClient,
};

use super::{Notification, Notifier};
use crate::state::AppState;

pub struct WebPushNotifier {
    enabled: bool,
    state: Arc<Mutex<AppState>>,
    vapid_private_key_path: String,
}

impl WebPushNotifier {
    pub fn new(enabled: bool, state: Arc<Mutex<AppState>>, vapid_private_key_path: String) -> Self {
        Self { enabled, state, vapid_private_key_path }
    }
}

#[async_trait]
impl Notifier for WebPushNotifier {
    async fn send(&self, notification: &Notification) -> Result<()> {
        let state = self.state.lock().await;
        if state.push_subscriptions.is_empty() {
            return Ok(());
        }

        let payload = serde_json::json!({
            "title": notification.title,
            "body": notification.body,
            "tag": format!("psst-{}-{}", notification.provider_id, notification.window_name),
        });
        let payload_str = payload.to_string();

        let client = IsahcWebPushClient::new()
            .context("Failed to create Web Push client")?;

        for sub in &state.push_subscriptions {
            let subscription = SubscriptionInfo::new(
                &sub.endpoint,
                &sub.keys.p256dh,
                &sub.keys.auth,
            );

            let sig_builder = VapidSignatureBuilder::from_pem_no_sub(
                std::fs::File::open(&self.vapid_private_key_path)
                    .context("Failed to open VAPID private key")?,
            )
            .context("Failed to build VAPID signature")?;

            let mut builder = WebPushMessageBuilder::new(&subscription);
            builder.set_payload(ContentEncoding::Aes128Gcm, payload_str.as_bytes());
            builder.set_vapid_signature(sig_builder.build()
                .context("Failed to sign VAPID")?);

            match client.send(builder.build().context("Failed to build push message")?).await {
                Ok(_) => tracing::debug!("Push sent to {}", &sub.endpoint[..50.min(sub.endpoint.len())]),
                Err(e) => tracing::warn!("Push failed for endpoint: {}", e),
            }
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "web_push"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}
```

- [ ] **Step 2: Add VAPID key generation to `psst init`**

Add to `cmd_init()` in `src/main.rs`, after creating state:

```rust
    // Generate VAPID keys if not present
    let vapid_private = dir.join("vapid_private.pem");
    let vapid_public = dir.join("vapid_public.pem");
    if !vapid_private.exists() {
        println!("Generating VAPID keys...");
        let status = std::process::Command::new("openssl")
            .args(["ecparam", "-genkey", "-name", "prime256v1", "-out"])
            .arg(&vapid_private)
            .status()?;
        if status.success() {
            let _ = std::process::Command::new("openssl")
                .args(["ec", "-in"])
                .arg(&vapid_private)
                .args(["-pubout", "-out"])
                .arg(&vapid_public)
                .status()?;
            println!("VAPID keys generated at {}", dir.display());
        } else {
            println!("Warning: Failed to generate VAPID keys (openssl not found?)");
            println!("PWA push notifications will not work without VAPID keys.");
        }
    }
```

- [ ] **Step 3: Add WebPushNotifier to cmd_run notifier list**

In `cmd_run`, add the web push notifier after building the scheduler (since it needs shared_state):

```rust
    // Add web push notifier (needs shared state)
    let web_push_enabled = config_notifications_web_push_enabled;
    let vapid_path = config_dir().join("vapid_private.pem").to_string_lossy().to_string();
    // Note: WebPushNotifier is added to dispatcher before starting scheduler
```

Actually, since WebPushNotifier needs shared state which is created inside Scheduler, we need to restructure slightly. Update `cmd_run` to create the state Arc before the scheduler:

The full updated `cmd_run` is provided in the commit — the key change is creating `Arc<Mutex<AppState>>` first, then passing it to both Scheduler and WebPushNotifier.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: Compiles (web-push crate may have some warnings, that's fine).

- [ ] **Step 5: Commit**

```bash
git add src/notifiers/web_push_notifier.rs src/main.rs
git commit -m "feat: add PWA Web Push notifier with VAPID support"
```

---

### Task 17: Integration Test & Polish

**Files:**
- Modify: `src/main.rs` (final wiring)
- Create: `tests/notifier_test.rs`

- [ ] **Step 1: Write integration test for notifier dispatch**

Create `tests/notifier_test.rs`:

```rust
use psst::notifiers::{format_notification, Notification};
use psst::threshold::{AlertEvent, AlertKind};
use chrono::{Duration, Utc};

#[test]
fn test_format_usage_threshold_notification() {
    let event = AlertEvent {
        provider_id: "claude".to_string(),
        window_name: "five_hour".to_string(),
        kind: AlertKind::UsageThreshold(80),
        utilization: 0.82,
        resets_at: Some(Utc::now() + Duration::hours(2)),
    };
    let n = format_notification(&event);
    assert!(n.title.contains("Psst!"));
    assert!(n.title.contains("Claude"));
    assert!(n.title.contains("80%"));
    assert!(n.body.contains("82%"));
    assert!(n.body.contains("18%"));
}

#[test]
fn test_format_reset_countdown_notification() {
    let event = AlertEvent {
        provider_id: "claude".to_string(),
        window_name: "seven_day".to_string(),
        kind: AlertKind::ResetCountdown(12),
        utilization: 0.30,
        resets_at: Some(Utc::now() + Duration::hours(11)),
    };
    let n = format_notification(&event);
    assert!(n.title.contains("Psst!"));
    assert!(n.title.contains("12小时"));
    assert!(n.title.contains("重置"));
    assert!(n.body.contains("30%"));
    assert!(n.body.contains("充分利用"));
}

#[test]
fn test_format_cursor_notification() {
    let event = AlertEvent {
        provider_id: "cursor".to_string(),
        window_name: "monthly".to_string(),
        kind: AlertKind::UsageThreshold(50),
        utilization: 0.52,
        resets_at: Some(Utc::now() + Duration::days(12)),
    };
    let n = format_notification(&event);
    assert!(n.title.contains("Cursor"));
    assert!(n.title.contains("50%"));
}
```

- [ ] **Step 2: Run all tests**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests PASS.

- [ ] **Step 3: Run the full build**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compiles successfully.

- [ ] **Step 4: Test the binary end-to-end**

Run:
```bash
./target/release/psst init
./target/release/psst status
```
Expected: Init creates config files, status shows "Last check: never".

- [ ] **Step 5: Commit**

```bash
git add tests/notifier_test.rs
git commit -m "feat: add integration tests for notification formatting"
```

- [ ] **Step 6: Final commit with LICENSE and README placeholder**

Create `LICENSE` (MIT):
```
MIT License

Copyright (c) 2026 Psst Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

```bash
git add LICENSE
git commit -m "chore: add MIT license"
```
