use anyhow::Result;
use async_trait::async_trait;

use super::{Notification, Notifier};

pub struct DesktopNotifier {
    pub enabled: bool,
}

impl DesktopNotifier {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
}

#[async_trait]
impl Notifier for DesktopNotifier {
    async fn send(&self, notification: &Notification) -> Result<()> {
        notify_rust::Notification::new()
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
