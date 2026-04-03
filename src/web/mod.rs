pub mod api;

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
}

impl WebServer {
    pub fn new(
        bind: String,
        state: Arc<Mutex<AppState>>,
        access_token: Option<String>,
    ) -> Self {
        Self {
            bind,
            state,
            access_token,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let ctx = AppContext {
            state: Arc::clone(&self.state),
            access_token: self.access_token.clone(),
        };

        let app = Router::new()
            .route("/",              get(api::get_index))
            .route("/manifest.json", get(api::get_manifest))
            .route("/sw.js",         get(api::get_sw))
            .route("/app.js",        get(api::get_app_js))
            .route("/api/health",    get(api::get_health))
            .route("/api/status",    get(api::get_status))
            .route("/api/subscribe", post(api::post_subscribe))
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
