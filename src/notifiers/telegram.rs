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

#[async_trait]
impl Notifier for TelegramNotifier {
    async fn send(&self, notification: &Notification) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.token
        );
        let text = format!("*{}*\n\n{}", notification.title, notification.body);
        let body = json!({
            "chat_id": self.chat_id,
            "text": text,
            "parse_mode": "Markdown"
        });

        let response = self.client.post(&url).json(&body).send().await?;

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
