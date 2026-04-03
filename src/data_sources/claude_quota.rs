//! Claude OAuth quota provider.
//!
//! Reads OAuth token from macOS Keychain (service: "Claude Code-credentials"),
//! falling back to `~/.claude/.credentials.json`, then calls
//! `GET https://api.anthropic.com/api/oauth/usage` to retrieve the current
//! five-hour and seven-day request windows.

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

use super::{QuotaInfo, QuotaProvider, QuotaWindow};

// ---------------------------------------------------------------------------
// Credentials — JSON structure stored in Keychain or file
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OAuthBlock>,
}

#[derive(Debug, Deserialize)]
struct OAuthBlock {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
}

// ---------------------------------------------------------------------------
// Token retrieval: Keychain first, file fallback
// ---------------------------------------------------------------------------

/// Read OAuth access token from macOS Keychain.
fn read_token_from_keychain() -> Result<String> {
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-s", "Claude Code-credentials",
            "-g",
        ])
        .output()
        .map_err(|e| anyhow!("Failed to run `security` command: {}", e))?;

    if !output.status.success() {
        bail!("No Claude Code credentials found in macOS Keychain");
    }

    // `security -g` prints the password to stderr in the format:
    // password: "{ JSON content }"
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Extract the JSON from the password field
    let json_str = extract_password_json(&stderr)
        .ok_or_else(|| anyhow!("Could not parse password from Keychain output"))?;

    let creds: CredentialsFile = serde_json::from_str(&json_str)
        .map_err(|e| anyhow!("Failed to parse Keychain credentials JSON: {}", e))?;

    creds
        .claude_ai_oauth
        .and_then(|oauth| oauth.access_token)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("No accessToken in Keychain credentials"))
}

/// Extract JSON string from `security -g` stderr output.
/// The password line looks like: password: "{ ... }" or password: 0x...
fn extract_password_json(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("password: \"") {
            // Remove 'password: "' prefix and trailing '"'
            let inner = &trimmed["password: \"".len()..];
            if let Some(json) = inner.strip_suffix('"') {
                // Unescape the string (security command escapes quotes)
                let unescaped = json.replace("\\\"", "\"").replace("\\\\", "\\");
                return Some(unescaped);
            }
        }
    }
    None
}

/// Read OAuth token from ~/.claude/.credentials.json (legacy/fallback).
fn read_token_from_file(home_dir: &str) -> Result<String> {
    let path = PathBuf::from(home_dir)
        .join(".claude")
        .join(".credentials.json");

    let content = std::fs::read_to_string(&path).map_err(|e| {
        anyhow!(
            "Cannot read Claude credentials at {}: {}",
            path.display(),
            e
        )
    })?;

    let creds: CredentialsFile = serde_json::from_str(&content)
        .map_err(|e| anyhow!("Failed to parse Claude credentials: {}", e))?;

    creds
        .claude_ai_oauth
        .and_then(|oauth| oauth.access_token)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("No accessToken in credentials file"))
}

/// Read OAuth token: try Keychain first, then file fallback.
fn read_oauth_token(home_dir: &str) -> Result<String> {
    match read_token_from_keychain() {
        Ok(token) => {
            tracing::debug!("Claude OAuth token read from macOS Keychain");
            Ok(token)
        }
        Err(keychain_err) => {
            tracing::debug!("Keychain read failed: {}, trying file fallback", keychain_err);
            read_token_from_file(home_dir).map_err(|file_err| {
                anyhow!(
                    "Cannot read Claude OAuth token.\n\
                     Keychain: {}\n\
                     File fallback: {}\n\
                     Please ensure Claude Code is logged in.",
                    keychain_err,
                    file_err
                )
            })
        }
    }
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

/// Raw response from `GET /api/oauth/usage`.
#[derive(Debug, Deserialize)]
struct OAuthUsageResponse {
    #[serde(default)]
    five_hour: Option<UsageWindowResponse>,
    #[serde(default)]
    seven_day: Option<UsageWindowResponse>,
}

#[derive(Debug, Deserialize)]
struct UsageWindowResponse {
    /// Utilization as a float 0.0 – 1.0 (newer API format)
    utilization: Option<f64>,
    /// Used count (older API format)
    used: Option<u64>,
    /// Limit count (older API format)
    limit: Option<u64>,
    /// ISO-8601 reset timestamp (may be `reset_at` or `resets_at`)
    reset_at: Option<String>,
    resets_at: Option<String>,
}

impl UsageWindowResponse {
    fn utilization_value(&self) -> f64 {
        // Prefer direct utilization field, fallback to used/limit ratio
        if let Some(u) = self.utilization {
            return u;
        }
        match (self.used, self.limit) {
            (Some(used), Some(limit)) if limit > 0 => used as f64 / limit as f64,
            _ => 0.0,
        }
    }

    fn reset_time(&self) -> Option<DateTime<Utc>> {
        self.resets_at
            .as_deref()
            .or(self.reset_at.as_deref())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

pub struct ClaudeQuotaProvider {
    home_dir: String,
    client: reqwest::Client,
}

impl ClaudeQuotaProvider {
    pub fn new(home_dir: impl Into<String>) -> Self {
        Self {
            home_dir: home_dir.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl QuotaProvider for ClaudeQuotaProvider {
    fn provider_id(&self) -> &str {
        "claude"
    }

    async fn fetch_quota(&self) -> Result<QuotaInfo> {
        let token = read_oauth_token(&self.home_dir)?;

        let response = self
            .client
            .get(USAGE_URL)
            .bearer_auth(&token)
            .header("anthropic-beta", "oauth-2025-04-20")
            .send()
            .await
            .map_err(|e| anyhow!("Network error fetching Claude quota: {}", e))?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            bail!(
                "Claude quota API returned 429 – rate limited. Will retry next cycle."
            );
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            bail!(
                "Claude OAuth token expired or invalid. Please run `claude login` to re-authenticate."
            );
        }

        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            bail!("Claude quota API returned {}: {}", status, body_text);
        }

        let body: OAuthUsageResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse Claude quota response: {}", e))?;

        let mut windows = Vec::new();

        if let Some(w) = &body.five_hour {
            windows.push(QuotaWindow {
                name: "five_hour".to_string(),
                utilization: w.utilization_value(),
                resets_at: w.reset_time().or_else(|| {
                    Some(Utc::now() + Duration::hours(5))
                }),
                used_tokens: None,
                used_count: w.used,
            });
        }

        if let Some(w) = &body.seven_day {
            windows.push(QuotaWindow {
                name: "seven_day".to_string(),
                utilization: w.utilization_value(),
                resets_at: w.reset_time().or_else(|| {
                    Some(Utc::now() + Duration::days(7))
                }),
                used_tokens: None,
                used_count: w.used,
            });
        }

        Ok(QuotaInfo {
            provider_id: "claude".to_string(),
            windows,
        })
    }
}
