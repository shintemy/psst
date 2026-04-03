use std::fs::File;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::{info, warn};
use web_push::{
    ContentEncoding, IsahcWebPushClient, SubscriptionInfo, VapidSignatureBuilder,
    WebPushClient, WebPushMessageBuilder,
};

use crate::notifiers::{Notification, Notifier};
use crate::state::AppState;

pub struct WebPushNotifier {
    enabled: bool,
    state: Arc<Mutex<AppState>>,
    vapid_private_key_path: String,
}

impl WebPushNotifier {
    pub fn new(
        enabled: bool,
        state: Arc<Mutex<AppState>>,
        vapid_private_key_path: String,
    ) -> Self {
        Self {
            enabled,
            state,
            vapid_private_key_path,
        }
    }
}

#[async_trait]
impl Notifier for WebPushNotifier {
    fn name(&self) -> &str {
        "web_push"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    async fn send(&self, notification: &Notification) -> Result<()> {
        let subscriptions = {
            let state = self.state.lock().await;
            state.push_subscriptions.clone()
        };

        if subscriptions.is_empty() {
            return Ok(());
        }

        // Build the payload JSON.
        let tag = format!(
            "psst-{}-{}",
            notification.provider_id, notification.window_name
        );
        let payload_json = serde_json::json!({
            "title": notification.title,
            "body": notification.body,
            "tag": tag,
        });
        let payload_bytes = serde_json::to_vec(&payload_json)?;

        // Open VAPID private key PEM file once (fail fast if missing).
        let pem_file = match File::open(&self.vapid_private_key_path) {
            Ok(f) => f,
            Err(e) => {
                warn!(
                    path = %self.vapid_private_key_path,
                    error = %e,
                    "WebPush: cannot open VAPID private key — skipping all subscriptions"
                );
                return Ok(());
            }
        };

        // Build a PartialVapidSignatureBuilder so we can clone it per subscription.
        let partial_builder = match VapidSignatureBuilder::from_pem_no_sub(pem_file) {
            Ok(b) => b,
            Err(e) => {
                warn!(error = %e, "WebPush: failed to parse VAPID private key");
                return Ok(());
            }
        };

        let client = match IsahcWebPushClient::new() {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "WebPush: failed to create HTTP client");
                return Ok(());
            }
        };

        for sub in &subscriptions {
            let subscription_info = SubscriptionInfo::new(
                &sub.endpoint,
                &sub.keys.p256dh,
                &sub.keys.auth,
            );

            // Build per-subscription VAPID signature.
            let vapid_signature = match partial_builder
                .clone()
                .add_sub_info(&subscription_info)
                .build()
            {
                Ok(sig) => sig,
                Err(e) => {
                    warn!(
                        endpoint = %sub.endpoint,
                        error = %e,
                        "WebPush: failed to build VAPID signature"
                    );
                    continue;
                }
            };

            let mut msg_builder = WebPushMessageBuilder::new(&subscription_info);
            msg_builder.set_payload(ContentEncoding::Aes128Gcm, &payload_bytes);
            msg_builder.set_vapid_signature(vapid_signature);

            let message = match msg_builder.build() {
                Ok(m) => m,
                Err(e) => {
                    warn!(
                        endpoint = %sub.endpoint,
                        error = %e,
                        "WebPush: failed to build WebPushMessage"
                    );
                    continue;
                }
            };

            match client.send(message).await {
                Ok(()) => {
                    info!(endpoint = %sub.endpoint, "WebPush: notification sent");
                }
                Err(e) => {
                    warn!(
                        endpoint = %sub.endpoint,
                        error = %e,
                        "WebPush: failed to send notification"
                    );
                }
            }
        }

        Ok(())
    }
}
