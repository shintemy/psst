pub mod cursor_local;
pub mod discovery;
pub mod estimated_quota;
pub mod usage_collector;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// A single window of quota usage (e.g. 5-hour window, 7-day window, daily).
#[derive(Debug, Clone)]
pub struct QuotaWindow {
    pub name: String,
    /// Fraction of quota consumed (0.0–1.0). Values > 1.0 mean over-limit.
    pub utilization: f64,
    pub resets_at: Option<DateTime<Utc>>,
    /// Raw tokens consumed (if applicable).
    pub used_tokens: Option<i64>,
    /// Raw request/message count consumed (if applicable).
    pub used_count: Option<u64>,
}

/// Quota information for a single provider.
#[derive(Debug, Clone)]
pub struct QuotaInfo {
    pub provider_id: String,
    pub windows: Vec<QuotaWindow>,
}

/// Trait implemented by each quota data source.
#[async_trait]
pub trait QuotaProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn fetch_quota(&self) -> Result<QuotaInfo>;
}
