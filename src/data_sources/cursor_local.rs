//! Cursor local usage provider.
//!
//! Reads usage data directly from Cursor's local SQLite database at
//! `~/.cursor/ai-tracking/ai-code-tracking.db`. Counts distinct requestIds
//! in the `ai_code_hashes` table within the current billing cycle.
//!
//! No remote API calls are made — this is purely local file reading.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{Datelike, Duration, TimeZone, Utc};
use std::path::PathBuf;

use super::{QuotaInfo, QuotaProvider, QuotaWindow};

pub struct CursorLocalProvider {
    home_dir: String,
    monthly_limit: u64,
    billing_day: u32,
}

impl CursorLocalProvider {
    pub fn new(home_dir: impl Into<String>, monthly_limit: u64, billing_day: u32) -> Self {
        Self {
            home_dir: home_dir.into(),
            monthly_limit,
            billing_day: billing_day.clamp(1, 28),
        }
    }

    fn db_path(&self) -> PathBuf {
        PathBuf::from(&self.home_dir)
            .join(".cursor")
            .join("ai-tracking")
            .join("ai-code-tracking.db")
    }

    /// Count distinct requests since the given timestamp (milliseconds).
    fn count_requests_since(&self, since_ms: i64) -> Result<u64> {
        let db_path = self.db_path();
        if !db_path.exists() {
            return Ok(0);
        }

        let conn = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("Failed to open Cursor DB: {}", db_path.display()))?;

        let count: u64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT requestId) FROM ai_code_hashes WHERE createdAt >= ?1 AND requestId IS NOT NULL AND requestId != ''",
                [since_ms],
                |row| row.get(0),
            )
            .with_context(|| "Failed to query Cursor usage")?;

        Ok(count)
    }
}

#[async_trait]
impl QuotaProvider for CursorLocalProvider {
    fn provider_id(&self) -> &str {
        "cursor"
    }

    async fn fetch_quota(&self) -> Result<QuotaInfo> {
        let now = Utc::now();
        let year = now.year();
        let month = now.month();
        let day = now.day();
        let billing_day = self.billing_day;

        // Determine billing cycle boundaries.
        let (cycle_start, cycle_end) = if day >= billing_day {
            let start = Utc
                .with_ymd_and_hms(year, month, billing_day, 0, 0, 0)
                .single()
                .unwrap_or(now);
            let (next_year, next_month) = if month == 12 {
                (year + 1, 1)
            } else {
                (year, month + 1)
            };
            let end = Utc
                .with_ymd_and_hms(next_year, next_month, billing_day, 0, 0, 0)
                .single()
                .unwrap_or(now + Duration::days(30));
            (start, end)
        } else {
            let (prev_year, prev_month) = if month == 1 {
                (year - 1, 12)
            } else {
                (year, month - 1)
            };
            let start = Utc
                .with_ymd_and_hms(prev_year, prev_month, billing_day, 0, 0, 0)
                .single()
                .unwrap_or(now - Duration::days(30));
            let end = Utc
                .with_ymd_and_hms(year, month, billing_day, 0, 0, 0)
                .single()
                .unwrap_or(now);
            (start, end)
        };

        let since_ms = cycle_start.timestamp_millis();
        let used = self.count_requests_since(since_ms)?;

        let utilization = if self.monthly_limit > 0 {
            used as f64 / self.monthly_limit as f64
        } else {
            0.0
        };

        Ok(QuotaInfo {
            provider_id: "cursor".to_string(),
            windows: vec![QuotaWindow {
                name: "monthly_requests".to_string(),
                utilization,
                resets_at: Some(cycle_end),
                used_tokens: None,
                used_count: Some(used),
            }],
        })
    }
}
