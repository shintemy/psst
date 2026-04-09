//! Cursor API usage provider.
//!
//! Reads JWT credentials from Cursor IDE's local state.vscdb,
//! refreshes expired tokens via OAuth, and calls the
//! aiserver.v1.DashboardService/GetCurrentPeriodUsage gRPC endpoint
//! for exact billing-cycle usage percentages.

use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use std::path::PathBuf;

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
