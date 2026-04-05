use anyhow::{bail, Result};
use async_trait::async_trait;
use std::process::Command;

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
        // Use osascript for reliable notifications on modern macOS.
        // notify-rust (NSUserNotification) is deprecated and silently dropped
        // on macOS 13+.
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            escape_applescript(&notification.body),
            escape_applescript(&notification.title),
        );
        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("osascript failed: {}", stderr.trim());
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "desktop"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Escape special characters for AppleScript string literals.
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
