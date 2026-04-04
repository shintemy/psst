use anyhow::{bail, Result};
use async_trait::async_trait;
use serde_json::json;

use super::{Notification, Notifier};

pub struct TelegramNotifier {
    pub token: String,
    pub chat_id: String,
    pub enabled: bool,
    client: reqwest::Client,
}

impl TelegramNotifier {
    pub fn new(token: String, chat_id: String, enabled: bool) -> Self {
        Self {
            token,
            chat_id,
            enabled,
            client: reqwest::Client::new(),
        }
    }
}

/// Escape special characters for Telegram MarkdownV2.
fn escape_mdv2(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "_*[]()~`>#+-=|{}.!".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[async_trait]
impl Notifier for TelegramNotifier {
    async fn send(&self, notification: &Notification) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.token
        );
        let title = escape_mdv2(&notification.title);
        let body = escape_mdv2(&notification.body);
        let text = format!("*{}*\n\n{}", title, body);
        let payload = json!({
            "chat_id": self.chat_id,
            "text": text,
            "parse_mode": "MarkdownV2"
        });

        let response = self.client.post(&url).json(&payload).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            bail!("Telegram API error {}: {}", status, text);
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "telegram"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}
