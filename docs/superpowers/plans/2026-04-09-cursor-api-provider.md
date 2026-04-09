# Cursor API Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Cursor's SQLite-based usage counting with Cursor's internal gRPC-over-HTTP API for exact usage percentages, with OAuth token auto-refresh.

**Architecture:** New `CursorApiProvider` reads JWT from Cursor's local `state.vscdb`, refreshes via OAuth when expired, and calls `aiserver.v1.DashboardService/GetCurrentPeriodUsage` for precise billing data. Falls back to existing `CursorLocalProvider` when Cursor IDE credentials are unavailable.

**Tech Stack:** Rust, reqwest (already dep), rusqlite (already dep), base64 (already dep), serde_json (already dep)

**Spec:** `docs/superpowers/specs/2026-04-09-cursor-api-provider-design.md`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/data_sources/cursor_api.rs` | Create | CursorApiProvider: token read, refresh, API call, QuotaProvider impl |
| `src/data_sources/mod.rs` | Modify | Add `pub mod cursor_api;` |
| `src/scheduler.rs` | Modify | Update `build_providers()` cursor branch to try API first, fallback to SQLite |
| `src/notifiers/mod.rs` | Modify | Add Chinese display names for `auto_requests`, `api_requests` |
| `src/web/static/app.js` | Modify | Add `auto_requests`, `api_requests` to `WINDOW_META` and `WINDOW_ORDER` |
| `tests/cursor_api_test.rs` | Create | Integration tests for token reading, JWT decoding, API response parsing |

---

### Task 1: JWT Token Reader

Read Cursor's access/refresh tokens from `state.vscdb`.

**Files:**
- Create: `src/data_sources/cursor_api.rs`
- Modify: `src/data_sources/mod.rs`
- Create: `tests/cursor_api_test.rs`

- [ ] **Step 1: Write failing test for token reading**

In `tests/cursor_api_test.rs`:

```rust
use psst::data_sources::cursor_api::read_cursor_tokens;
use tempfile::TempDir;

#[test]
fn test_read_cursor_tokens_from_vscdb() {
    let dir = TempDir::new().unwrap();
    let db_path = dir
        .path()
        .join("Library/Application Support/Cursor/User/globalStorage/state.vscdb");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO ItemTable VALUES ('cursorAuth/accessToken', 'test-access-token');
         INSERT INTO ItemTable VALUES ('cursorAuth/refreshToken', 'test-refresh-token');",
    )
    .unwrap();

    let tokens = read_cursor_tokens(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(tokens.access_token, "test-access-token");
    assert_eq!(tokens.refresh_token, "test-refresh-token");
}

#[test]
fn test_read_cursor_tokens_missing_db() {
    let dir = TempDir::new().unwrap();
    let result = read_cursor_tokens(dir.path().to_str().unwrap());
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test cursor_api_test`
Expected: FAIL — module `cursor_api` not found

- [ ] **Step 3: Implement token reader**

In `src/data_sources/mod.rs`, add the module declaration after line 3:

```rust
pub mod cursor_api;
```

Create `src/data_sources/cursor_api.rs`:

```rust
//! Cursor API usage provider.
//!
//! Reads JWT credentials from Cursor IDE's local state.vscdb,
//! refreshes expired tokens via OAuth, and calls the
//! aiserver.v1.DashboardService/GetCurrentPeriodUsage gRPC endpoint
//! for exact billing-cycle usage percentages.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// JWT credentials read from Cursor's state.vscdb.
pub struct CursorTokens {
    pub access_token: String,
    pub refresh_token: String,
}

/// Path to Cursor's state.vscdb relative to a home directory.
fn vscdb_path(home_dir: &str) -> PathBuf {
    PathBuf::from(home_dir)
        .join("Library/Application Support/Cursor/User/globalStorage/state.vscdb")
}

/// Read access and refresh tokens from Cursor IDE's local SQLite store.
pub fn read_cursor_tokens(home_dir: &str) -> Result<CursorTokens> {
    let db_path = vscdb_path(home_dir);
    if !db_path.exists() {
        anyhow::bail!("Cursor state.vscdb not found at {}", db_path.display());
    }

    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("Failed to open Cursor state.vscdb: {}", db_path.display()))?;

    let access_token: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'",
            [],
            |row| row.get(0),
        )
        .with_context(|| "cursorAuth/accessToken not found in state.vscdb")?;

    let refresh_token: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'cursorAuth/refreshToken'",
            [],
            |row| row.get(0),
        )
        .with_context(|| "cursorAuth/refreshToken not found in state.vscdb")?;

    Ok(CursorTokens {
        access_token,
        refresh_token,
    })
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test cursor_api_test`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add src/data_sources/cursor_api.rs src/data_sources/mod.rs tests/cursor_api_test.rs
git commit -m "feat(cursor-api): add JWT token reader from state.vscdb"
```

---

### Task 2: JWT Expiry Check and OAuth Refresh

Decode JWT `exp` claim and refresh when near expiry.

**Files:**
- Modify: `src/data_sources/cursor_api.rs`
- Modify: `tests/cursor_api_test.rs`

- [ ] **Step 1: Write failing test for JWT expiry check**

Append to `tests/cursor_api_test.rs`:

```rust
use psst::data_sources::cursor_api::is_token_expired;

#[test]
fn test_expired_jwt() {
    // JWT with exp = 1000000000 (2001-09-09) — long expired
    // Header: {"alg":"HS256","typ":"JWT"}, Payload: {"sub":"test","exp":1000000000}
    let expired_jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
        eyJzdWIiOiJ0ZXN0IiwiZXhwIjoxMDAwMDAwMDAwfQ.\
        signature";
    assert!(is_token_expired(expired_jwt));
}

#[test]
fn test_not_expired_jwt() {
    // JWT with exp = 4102444800 (2100-01-01) — far future
    // Payload: {"sub":"test","exp":4102444800}
    let future_jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
        eyJzdWIiOiJ0ZXN0IiwiZXhwIjo0MTAyNDQ0ODAwfQ.\
        signature";
    assert!(!is_token_expired(future_jwt));
}

#[test]
fn test_malformed_jwt_treated_as_expired() {
    assert!(is_token_expired("not-a-jwt"));
    assert!(is_token_expired("only.two"));
    assert!(is_token_expired("a.b.c")); // invalid base64 payload
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test cursor_api_test test_expired_jwt test_not_expired_jwt test_malformed_jwt`
Expected: FAIL — `is_token_expired` not found

- [ ] **Step 3: Implement JWT expiry check**

Add to `src/data_sources/cursor_api.rs`:

```rust
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

/// Decode a JWT payload (no signature verification) and check if expired.
/// Returns `true` if the token is expired, malformed, or will expire within 5 minutes.
pub fn is_token_expired(jwt: &str) -> bool {
    let parts: Vec<&str> = jwt.splitn(3, '.').collect();
    if parts.len() < 2 {
        return true;
    }

    // Decode the payload (second segment). Try both URL-safe and standard base64.
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(parts[1]));
    let payload_bytes = match payload_bytes {
        Ok(b) => b,
        Err(_) => return true,
    };

    let payload: serde_json::Value = match serde_json::from_slice(&payload_bytes) {
        Ok(v) => v,
        Err(_) => return true,
    };

    let exp = match payload.get("exp").and_then(|v| v.as_i64()) {
        Some(e) => e,
        None => return true,
    };

    let now = chrono::Utc::now().timestamp();
    // Treat as expired if within 5 minutes of expiry.
    exp < now + 300
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test cursor_api_test`
Expected: PASS (5 tests)

- [ ] **Step 5: Write failing test for OAuth refresh response parsing**

Append to `tests/cursor_api_test.rs`:

```rust
use psst::data_sources::cursor_api::parse_refresh_response;

#[test]
fn test_parse_refresh_response_success() {
    let body = r#"{"access_token":"new-token","id_token":"id","shouldLogout":false}"#;
    let result = parse_refresh_response(body).unwrap();
    assert_eq!(result, "new-token");
}

#[test]
fn test_parse_refresh_response_logout() {
    let body = r#"{"access_token":"new-token","shouldLogout":true}"#;
    let result = parse_refresh_response(body);
    assert!(result.is_err());
}
```

- [ ] **Step 6: Run test to verify it fails**

Run: `cargo test --test cursor_api_test test_parse_refresh`
Expected: FAIL — `parse_refresh_response` not found

- [ ] **Step 7: Implement refresh response parser**

Add to `src/data_sources/cursor_api.rs`:

```rust
const OAUTH_TOKEN_URL: &str = "https://api2.cursor.sh/oauth/token";
const OAUTH_CLIENT_ID: &str = "KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB";

/// Parse the OAuth token refresh response. Returns the new access token,
/// or an error if `shouldLogout` is true or the response is malformed.
pub fn parse_refresh_response(body: &str) -> Result<String> {
    let data: serde_json::Value =
        serde_json::from_str(body).context("Invalid JSON in refresh response")?;

    if data.get("shouldLogout").and_then(|v| v.as_bool()).unwrap_or(false) {
        anyhow::bail!("Cursor server requested logout — refresh token invalidated");
    }

    data.get("access_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .context("No access_token in refresh response")
}

/// Refresh the access token using the refresh token.
/// Returns the new access token on success.
pub async fn refresh_access_token(refresh_token: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(OAUTH_TOKEN_URL)
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "client_id": OAUTH_CLIENT_ID,
            "refresh_token": refresh_token,
        }))
        .send()
        .await
        .context("Failed to reach Cursor OAuth endpoint")?;

    let status = resp.status();
    let body = resp.text().await.context("Failed to read refresh response body")?;

    if !status.is_success() {
        anyhow::bail!("OAuth refresh failed (HTTP {}): {}", status, body);
    }

    parse_refresh_response(&body)
}
```

- [ ] **Step 8: Run test to verify it passes**

Run: `cargo test --test cursor_api_test`
Expected: PASS (7 tests)

- [ ] **Step 9: Commit**

```bash
git add src/data_sources/cursor_api.rs tests/cursor_api_test.rs
git commit -m "feat(cursor-api): add JWT expiry check and OAuth refresh"
```

---

### Task 3: API Response Parsing and QuotaProvider Implementation

Parse the `GetCurrentPeriodUsage` response and implement the `QuotaProvider` trait.

**Files:**
- Modify: `src/data_sources/cursor_api.rs`
- Modify: `tests/cursor_api_test.rs`

- [ ] **Step 1: Write failing test for API response parsing**

Append to `tests/cursor_api_test.rs`:

```rust
use psst::data_sources::cursor_api::parse_usage_response;

#[test]
fn test_parse_usage_response() {
    let body = r#"{
        "billingCycleStart": "1773133068000",
        "billingCycleEnd": "1775811468000",
        "planUsage": {
            "totalSpend": 10455,
            "includedSpend": 10455,
            "remaining": 29545,
            "limit": 40000,
            "autoPercentUsed": 0.175,
            "apiPercentUsed": 20.56,
            "totalPercentUsed": 6.97
        },
        "enabled": true,
        "displayMessage": "You've used 26% of your included usage"
    }"#;

    let usage = parse_usage_response(body).unwrap();
    assert!((usage.total_percent - 6.97).abs() < 0.01);
    assert!((usage.auto_percent - 0.175).abs() < 0.01);
    assert!((usage.api_percent - 20.56).abs() < 0.01);
    assert_eq!(usage.billing_cycle_end_ms, 1775811468000);
}

#[test]
fn test_parse_usage_response_missing_plan_usage() {
    let body = r#"{"billingCycleStart":"0","billingCycleEnd":"0","enabled":false}"#;
    let result = parse_usage_response(body);
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test cursor_api_test test_parse_usage`
Expected: FAIL — `parse_usage_response` not found

- [ ] **Step 3: Implement response parser**

Add to `src/data_sources/cursor_api.rs`:

```rust
/// Parsed usage data from the Cursor API.
pub struct CursorUsage {
    pub total_percent: f64,
    pub auto_percent: f64,
    pub api_percent: f64,
    pub billing_cycle_end_ms: i64,
}

const USAGE_URL: &str =
    "https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage";

/// Parse the GetCurrentPeriodUsage JSON response.
pub fn parse_usage_response(body: &str) -> Result<CursorUsage> {
    let data: serde_json::Value =
        serde_json::from_str(body).context("Invalid JSON in usage response")?;

    let plan = data
        .get("planUsage")
        .context("No planUsage in usage response")?;

    let total_percent = plan
        .get("totalPercentUsed")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let auto_percent = plan
        .get("autoPercentUsed")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let api_percent = plan
        .get("apiPercentUsed")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    // billingCycleEnd comes as a string of milliseconds (e.g. "1775811468000")
    let billing_cycle_end_ms = data
        .get("billingCycleEnd")
        .and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<i64>().ok())
                .or_else(|| v.as_i64())
        })
        .unwrap_or(0);

    Ok(CursorUsage {
        total_percent,
        auto_percent,
        api_percent,
        billing_cycle_end_ms,
    })
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test cursor_api_test`
Expected: PASS (9 tests)

- [ ] **Step 5: Implement QuotaProvider**

Add to `src/data_sources/cursor_api.rs`:

```rust
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};

use super::{QuotaInfo, QuotaProvider, QuotaWindow};

pub struct CursorApiProvider {
    home_dir: String,
}

impl CursorApiProvider {
    pub fn new(home_dir: impl Into<String>) -> Self {
        Self {
            home_dir: home_dir.into(),
        }
    }

    /// Get a valid access token, refreshing if needed.
    async fn get_access_token(&self) -> Result<String> {
        let tokens = read_cursor_tokens(&self.home_dir)?;

        if !is_token_expired(&tokens.access_token) {
            return Ok(tokens.access_token);
        }

        tracing::info!("Cursor access token expired, refreshing via OAuth");
        refresh_access_token(&tokens.refresh_token).await
    }

    /// Call the GetCurrentPeriodUsage endpoint.
    async fn fetch_usage(&self, access_token: &str) -> Result<CursorUsage> {
        let client = reqwest::Client::new();
        let resp = client
            .post(USAGE_URL)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .body("{}")
            .send()
            .await
            .context("Failed to reach Cursor usage API")?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .context("Failed to read usage response body")?;

        if !status.is_success() {
            anyhow::bail!("Cursor usage API returned HTTP {}: {}", status, body);
        }

        parse_usage_response(&body)
    }
}

#[async_trait]
impl QuotaProvider for CursorApiProvider {
    fn provider_id(&self) -> &str {
        "cursor"
    }

    async fn fetch_quota(&self) -> Result<QuotaInfo> {
        let token = self.get_access_token().await?;
        let usage = self.fetch_usage(&token).await?;

        let resets_at = if usage.billing_cycle_end_ms > 0 {
            Utc.timestamp_millis_opt(usage.billing_cycle_end_ms).single()
        } else {
            None
        };

        Ok(QuotaInfo {
            provider_id: "cursor".to_string(),
            windows: vec![
                QuotaWindow {
                    name: "monthly_requests".to_string(),
                    utilization: usage.total_percent / 100.0,
                    resets_at,
                    used_tokens: None,
                    used_count: None,
                },
                QuotaWindow {
                    name: "auto_requests".to_string(),
                    utilization: usage.auto_percent / 100.0,
                    resets_at,
                    used_tokens: None,
                    used_count: None,
                },
                QuotaWindow {
                    name: "api_requests".to_string(),
                    utilization: usage.api_percent / 100.0,
                    resets_at,
                    used_tokens: None,
                    used_count: None,
                },
            ],
        })
    }
}
```

- [ ] **Step 6: Run all tests**

Run: `cargo test --test cursor_api_test`
Expected: PASS (9 tests)

Run: `cargo check`
Expected: compiles without errors

- [ ] **Step 7: Commit**

```bash
git add src/data_sources/cursor_api.rs tests/cursor_api_test.rs
git commit -m "feat(cursor-api): implement QuotaProvider with usage API"
```

---

### Task 4: Wire Into Scheduler with Fallback

Update `build_providers()` to use `CursorApiProvider` as primary, `CursorLocalProvider` as fallback.

**Files:**
- Modify: `src/scheduler.rs:1-5` (imports), `src/scheduler.rs:192-203` (cursor branch)

- [ ] **Step 1: Update scheduler imports**

In `src/scheduler.rs`, add the import at line 8 (after existing `use crate::data_sources::cursor_local::CursorLocalProvider;`):

```rust
use crate::data_sources::cursor_api::CursorApiProvider;
```

- [ ] **Step 2: Update the cursor branch in build_providers()**

Replace lines 192–203 of `src/scheduler.rs` (the `"cursor" =>` arm):

```rust
                "cursor" => {
                    // Try the API provider first (reads JWT from Cursor IDE's
                    // local state.vscdb for exact billing percentages).
                    // Fall back to SQLite request counting if Cursor IDE
                    // credentials are not available.
                    let api_available =
                        crate::data_sources::cursor_api::read_cursor_tokens(&self.home_dir)
                            .is_ok();

                    if api_available {
                        providers.push(Box::new(CursorApiProvider::new(
                            self.home_dir.clone(),
                        )));
                    } else if let Some(limit) = provider_config.monthly_fast_requests {
                        let billing_day = provider_config.billing_day.unwrap_or(1);
                        providers.push(Box::new(CursorLocalProvider::new(
                            self.home_dir.clone(),
                            limit,
                            billing_day,
                        )));
                    }
                }
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: compiles without errors

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: all existing tests pass (ignore pre-existing notifier_test failures)

- [ ] **Step 5: Commit**

```bash
git add src/scheduler.rs
git commit -m "feat(cursor-api): wire CursorApiProvider into scheduler with fallback"
```

---

### Task 5: Update Dashboard and Notification Labels

Add display names for the two new window types.

**Files:**
- Modify: `src/web/static/app.js:32-42`
- Modify: `src/notifiers/mod.rs:65-74`

- [ ] **Step 1: Update dashboard WINDOW_META and WINDOW_ORDER**

In `src/web/static/app.js`, replace lines 32–42:

```javascript
  const WINDOW_META = {
    'monthly_requests': { label: 'Monthly Requests', est: false },
    'auto_requests':    { label: 'Auto Models',      est: false },
    'api_requests':     { label: 'API Models',       est: false },
    'weekly_requests':  { label: 'Weekly Budget',    est: true  },
    'daily_requests':   { label: 'Daily Budget',     est: true  },
    'daily_tokens':     { label: 'Daily Tokens',     est: false },
    'five_hour':        { label: '5-Hour Window',    est: false },
    'seven_day':        { label: '7-Day Window',     est: false },
  };

  // Sort order: monthly first, then auto/api, weekly, daily, others
  const WINDOW_ORDER = ['monthly_requests', 'auto_requests', 'api_requests', 'weekly_requests', 'daily_requests', 'daily_tokens', 'five_hour', 'seven_day'];
```

- [ ] **Step 2: Update notification Chinese display names**

In `src/notifiers/mod.rs`, replace lines 65–74:

```rust
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
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add src/web/static/app.js src/notifiers/mod.rs
git commit -m "feat(cursor-api): add Auto/API window labels to dashboard and notifications"
```

---

### Task 6: Smoke Test — End-to-End Verification

Run the full system with real Cursor credentials and verify output.

**Files:** None modified — manual verification only.

- [ ] **Step 1: Run psst check and verify Cursor data**

Run: `cargo run -- run`

Wait for the first check cycle to complete (look for "State saved" in logs). Then in another terminal:

Run: `cargo run -- status`

Expected output should show cursor provider with 3 windows:
- `monthly_requests` — utilization should be close to Cursor's "Total: X%"
- `auto_requests` — should match Cursor's "X% Auto"
- `api_requests` — should match Cursor's "X% API"

- [ ] **Step 2: Verify dashboard renders correctly**

Open `http://127.0.0.1:3377?token=<your-token>` in a browser.

Verify:
- Cursor card shows 3 bars: "Monthly Requests", "Auto Models", "API Models"
- Percentages match Cursor's own settings page
- No "est." badge on any of the 3 windows (they are exact API data)

- [ ] **Step 3: Commit any final adjustments**

If any adjustments were needed during smoke testing, commit them.

```bash
git add -A
git commit -m "fix(cursor-api): adjustments from smoke testing"
```
