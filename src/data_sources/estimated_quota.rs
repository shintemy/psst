//! Generic quota estimation for tools with user-configured limits.
//!
//! Supports:
//! - `monthly_fast_requests`: counts messages in the current billing cycle.
//!   Automatically derives weekly and daily request windows from the monthly budget.
//! - `daily_token_limit`: counts tokens consumed today.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{Datelike, Duration, TimeZone, Utc};

use crate::config::ProviderConfig;

use super::{
    usage_collector::{collect_usage_since, collect_usage_today},
    QuotaInfo, QuotaProvider, QuotaWindow,
};

pub struct EstimatedQuotaProvider {
    provider_id: String,
    home_dir: String,
    config: ProviderConfig,
}

impl EstimatedQuotaProvider {
    pub fn new(
        provider_id: impl Into<String>,
        home_dir: impl Into<String>,
        config: ProviderConfig,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            home_dir: home_dir.into(),
            config,
        }
    }
}

#[async_trait]
impl QuotaProvider for EstimatedQuotaProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    async fn fetch_quota(&self) -> Result<QuotaInfo> {
        let mut windows = Vec::new();

        // ------------------------------------------------------------------
        // Monthly fast-request quota
        // ------------------------------------------------------------------
        if let Some(limit) = self.config.monthly_fast_requests {
            let billing_day = self.config.billing_day.unwrap_or(1).clamp(1, 28);

            // Determine start of current billing cycle.
            let now = Utc::now();
            let year = now.year();
            let month = now.month();
            let day = now.day();

            let (cycle_start, cycle_end) = if day >= billing_day {
                // We're in the cycle that started this month.
                let start = Utc
                    .with_ymd_and_hms(year, month, billing_day, 0, 0, 0)
                    .single()
                    .unwrap_or(now);
                // Next cycle starts next month.
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
                // Cycle started last month.
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

            let summary =
                collect_usage_since(&self.home_dir, &self.provider_id, cycle_start).await?;
            let used = summary.message_count as u64;
            let utilization = if limit > 0 {
                used as f64 / limit as f64
            } else {
                0.0
            };

            windows.push(QuotaWindow {
                name: "monthly_requests".to_string(),
                utilization,
                resets_at: Some(cycle_end),
                used_tokens: None,
                used_count: Some(used),
            });

            // ------------------------------------------------------------------
            // Weekly requests — estimated from monthly budget
            // NOTE: This is a rough pacing guide, not an official provider limit.
            // Actual provider limits (e.g. Claude) may use weighted factors
            // (message length, tool usage, etc.) rather than simple request counts.
            // Budget ≈ monthly / 4.33;  count = requests in last 7 days
            // ------------------------------------------------------------------
            let weekly_limit = (limit as f64 / 4.33).round() as u64;
            if weekly_limit > 0 {
                let seven_days_ago = now - Duration::days(7);
                let weekly_summary =
                    collect_usage_since(&self.home_dir, &self.provider_id, seven_days_ago).await?;
                let weekly_used = weekly_summary.message_count as u64;
                let weekly_utilization = weekly_used as f64 / weekly_limit as f64;

                // Rolling window: resets 7 days from now (approximate)
                let weekly_reset = now + Duration::days(7);

                windows.push(QuotaWindow {
                    name: "weekly_requests".to_string(),
                    utilization: weekly_utilization,
                    resets_at: Some(weekly_reset),
                    used_tokens: None,
                    used_count: Some(weekly_used),
                });
            }

            // ------------------------------------------------------------------
            // Daily requests — estimated from monthly budget (pacing guide)
            // Budget ≈ monthly / 30;  count = requests today
            // ------------------------------------------------------------------
            let daily_limit = (limit as f64 / 30.0).round() as u64;
            if daily_limit > 0 {
                let daily_summary =
                    collect_usage_today(&self.home_dir, &self.provider_id).await?;
                let daily_used = daily_summary.message_count as u64;
                let daily_utilization = daily_used as f64 / daily_limit as f64;

                let tomorrow_midnight_req = (now + Duration::days(1))
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .map(|dt| chrono::DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
                    .unwrap_or(now + Duration::days(1));

                windows.push(QuotaWindow {
                    name: "daily_requests".to_string(),
                    utilization: daily_utilization,
                    resets_at: Some(tomorrow_midnight_req),
                    used_tokens: None,
                    used_count: Some(daily_used),
                });
            }
        }

        // ------------------------------------------------------------------
        // Daily token quota
        // ------------------------------------------------------------------
        if let Some(limit) = self.config.daily_token_limit {
            let summary = collect_usage_today(&self.home_dir, &self.provider_id).await?;
            let used = summary.total_tokens;
            let utilization = if limit > 0 {
                used as f64 / limit as f64
            } else {
                0.0
            };

            // Daily window resets at midnight UTC.
            let tomorrow_midnight = (Utc::now() + Duration::days(1))
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|dt| chrono::DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));

            windows.push(QuotaWindow {
                name: "daily_tokens".to_string(),
                utilization,
                resets_at: tomorrow_midnight,
                used_tokens: Some(used),
                used_count: None,
            });
        }

        Ok(QuotaInfo {
            provider_id: self.provider_id.clone(),
            windows,
        })
    }
}
