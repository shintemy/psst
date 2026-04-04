pub mod api;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    routing::{get, post},
    Router,
};
use tokio::sync::Mutex;
use tracing::info;

use crate::state::AppState;
use api::AppContext;

pub struct WebServer {
    bind: String,
    state: Arc<Mutex<AppState>>,
    access_token: Option<String>,
    config_path: PathBuf,
    vapid_public_key_path: PathBuf,
}

impl WebServer {
    pub fn new(
        bind: String,
        state: Arc<Mutex<AppState>>,
        access_token: Option<String>,
        config_path: PathBuf,
        vapid_public_key_path: PathBuf,
    ) -> Self {
        Self {
            bind,
            state,
            access_token,
            config_path,
            vapid_public_key_path,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let ctx = AppContext {
            state: Arc::clone(&self.state),
            access_token: self.access_token.clone(),
            config_path: self.config_path.clone(),
            vapid_public_key_path: self.vapid_public_key_path.clone(),
        };

        let app = Router::new()
            .route("/",              get(api::get_index))
            .route("/manifest.json", get(api::get_manifest))
            .route("/sw.js",         get(api::get_sw))
            .route("/app.js",        get(api::get_app_js))
            .route("/api/health",    get(api::get_health))
            .route("/api/status",    get(api::get_status))
            .route("/api/config",    get(api::get_config).post(api::post_config))
            .route("/api/subscribe", post(api::post_subscribe))
            .route("/api/vapid-public-key", get(api::get_vapid_public_key))
            .with_state(ctx);

        let listener = tokio::net::TcpListener::bind(&self.bind)
            .await
            .with_context(|| format!("Failed to bind web server to {}", self.bind))?;

        info!("Web server listening on http://{}", self.bind);

        axum::serve(listener, app)
            .await
            .context("Web server error")?;

        Ok(())
    }
}
