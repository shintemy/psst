use anyhow::{bail, Result};
use async_trait::async_trait;

use super::{Notification, Notifier};

pub struct ServerChanNotifier {
    pub send_key: String,
    pub enabled: bool,
    client: reqwest::Client,
}

impl ServerChanNotifier {
    pub fn new(send_key: String, enabled: bool) -> Self {
        Self {
            send_key,
            enabled,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Notifier for ServerChanNotifier {
    async fn send(&self, notification: &Notification) -> Result<()> {
        let url = format!("https://sctapi.ftqq.com/{}.send", self.send_key);
        let params = [
            ("title", notification.title.as_str()),
            ("desp", notification.body.as_str()),
        ];

        let response = self.client.post(&url).form(&params).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            bail!("Server酱 API error {}: {}", status, text);
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "serverchan"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}
