//! Cursor API usage provider.
//!
//! Reads JWT credentials from Cursor IDE's local state.vscdb,
//! refreshes expired tokens via OAuth, and calls the
//! aiserver.v1.DashboardService/GetCurrentPeriodUsage gRPC endpoint
//! for exact billing-cycle usage percentages.

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::TimeZone;
use std::path::PathBuf;

use super::{QuotaInfo, QuotaProvider, QuotaWindow};

/// JWT credentials read from Cursor's state.vscdb.
#[derive(Debug, Clone)]
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

pub struct CursorApiProvider {
    home_dir: String,
    client: reqwest::Client,
}

impl CursorApiProvider {
    pub fn new(home_dir: impl Into<String>) -> Self {
        Self {
            home_dir: home_dir.into(),
            client: reqwest::Client::new(),
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
        let resp = self
            .client
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
            chrono::Utc
                .timestamp_millis_opt(usage.billing_cycle_end_ms)
                .single()
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
