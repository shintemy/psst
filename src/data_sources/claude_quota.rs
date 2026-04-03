//! Claude OAuth quota provider.
//!
//! Reads `~/.claude/.credentials.json` for an OAuth bearer token, then calls
//! `GET https://api.anthropic.com/api/oauth/usage` to retrieve the current
//! five-hour and seven-day request windows.

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use std::path::PathBuf;

use super::{QuotaInfo, QuotaProvider, QuotaWindow};

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ClaudeCredentials {
    #[serde(rename = "claudeAiOauthToken")]
    claude_ai_oauth_token: Option<String>,
    /// Fallback: some versions store the token under a different key.
    #[serde(rename = "oauth_token")]
    oauth_token: Option<String>,
}

fn read_oauth_token(home_dir: &str) -> Result<String> {
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

    let creds: ClaudeCredentials = serde_json::from_str(&content)
        .map_err(|e| anyhow!("Failed to parse Claude credentials: {}", e))?;

    creds
        .claude_ai_oauth_token
        .or(creds.oauth_token)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("No OAuth token found in ~/.claude/.credentials.json"))
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

/// Raw response from `GET /api/oauth/usage`.
#[derive(Debug, Deserialize)]
struct OAuthUsageResponse {
    #[serde(default)]
    five_hour: Option<UsageWindow>,
    #[serde(default)]
    seven_day: Option<UsageWindow>,
}

#[derive(Debug, Deserialize)]
struct UsageWindow {
    used: Option<u64>,
    limit: Option<u64>,
    /// ISO-8601 reset timestamp.
    reset_at: Option<String>,
}

impl UsageWindow {
    fn utilization(&self) -> f64 {
        match (self.used, self.limit) {
            (Some(used), Some(limit)) if limit > 0 => used as f64 / limit as f64,
            _ => 0.0,
        }
    }

    fn resets_at(&self) -> Option<DateTime<Utc>> {
        self.reset_at
            .as_deref()
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
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .map_err(|e| anyhow!("Network error fetching Claude quota: {}", e))?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            bail!(
                "Claude quota API returned 429 – rate limited. \
                 The quota check itself is being throttled; try again later."
            );
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            bail!(
                "Claude quota API returned 401 – OAuth token is invalid or expired. \
                 Please re-authenticate with `claude login`."
            );
        }

        if !status.is_success() {
            bail!("Claude quota API returned unexpected status: {}", status);
        }

        let body: OAuthUsageResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse Claude quota response: {}", e))?;

        let mut windows = Vec::new();

        if let Some(w) = &body.five_hour {
            windows.push(QuotaWindow {
                name: "five_hour".to_string(),
                utilization: w.utilization(),
                resets_at: w.resets_at().or_else(|| {
                    // Fallback: 5 hours from now if reset_at not provided.
                    Some(Utc::now() + Duration::hours(5))
                }),
                used_tokens: None,
                used_count: w.used.map(|u| u as u64),
            });
        }

        if let Some(w) = &body.seven_day {
            windows.push(QuotaWindow {
                name: "seven_day".to_string(),
                utilization: w.utilization(),
                resets_at: w.resets_at().or_else(|| {
                    Some(Utc::now() + Duration::days(7))
                }),
                used_tokens: None,
                used_count: w.used.map(|u| u as u64),
            });
        }

        Ok(QuotaInfo {
            provider_id: "claude".to_string(),
            windows,
        })
    }
}
