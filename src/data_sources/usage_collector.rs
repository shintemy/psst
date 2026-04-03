//! Collect historical token usage for a given tool via tokscale-core.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use tokscale_core::{parse_local_unified_messages, LocalParseOptions};

/// Aggregated usage over a time window.
#[derive(Debug, Clone, Default)]
pub struct UsageSummary {
    /// Sum of all token fields (input + output + cache_read + cache_write + reasoning).
    pub total_tokens: i64,
    /// Sum of cost in USD.
    pub total_cost: f64,
    /// Number of individual messages/API calls recorded.
    pub message_count: i64,
}

/// Collect usage for `tool_id` from `home_dir` since `since`.
///
/// `since` is an ISO-8601 / RFC-3339 timestamp; tokscale-core accepts
/// date strings like "2024-01-01" or full timestamps.
pub async fn collect_usage_since(
    home_dir: &str,
    tool_id: &str,
    since: DateTime<Utc>,
) -> Result<UsageSummary> {
    // Format as a date-time string that tokscale-core understands.
    let since_str = since.format("%Y-%m-%d").to_string();

    let options = LocalParseOptions {
        home_dir: Some(home_dir.to_string()),
        use_env_roots: false,
        clients: Some(vec![tool_id.to_string()]),
        since: Some(since_str),
        until: None,
        year: None,
    };

    let messages = parse_local_unified_messages(options)
        .await
        .map_err(|e| anyhow!("tokscale-core parse error for {}: {}", tool_id, e))?;

    let mut summary = UsageSummary::default();
    for msg in &messages {
        summary.total_tokens += msg.tokens.total();
        summary.total_cost += msg.cost;
        summary.message_count += msg.message_count as i64;
    }

    Ok(summary)
}

/// Collect usage for `tool_id` from `home_dir` over the current calendar day (UTC).
pub async fn collect_usage_today(home_dir: &str, tool_id: &str) -> Result<UsageSummary> {
    let today = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight always valid");
    let since = DateTime::<Utc>::from_naive_utc_and_offset(today, Utc);
    collect_usage_since(home_dir, tool_id, since).await
}
