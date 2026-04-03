use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::state::{AppState, PushKeys, PushSubscription};

// ── Shared application context ─────────────────────────────────────────────

#[derive(Clone)]
pub struct AppContext {
    pub state: Arc<Mutex<AppState>>,
    pub access_token: Option<String>,
}

// ── Token authentication helper ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TokenQuery {
    token: Option<String>,
}

fn check_token(ctx: &AppContext, query_token: Option<&str>) -> bool {
    match &ctx.access_token {
        None => true,
        Some(expected) => query_token.map(|t| t == expected).unwrap_or(false),
    }
}

// ── Static file helpers ────────────────────────────────────────────────────

fn html_response(body: &'static str) -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}

fn js_response(body: &'static str) -> Response {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        body,
    )
        .into_response()
}

fn json_str_response(body: &'static str) -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response()
}

// ── Route handlers ─────────────────────────────────────────────────────────

/// GET / — serve dashboard HTML (token-gated)
pub async fn get_index(
    State(ctx): State<AppContext>,
    Query(q): Query<TokenQuery>,
) -> Response {
    if !check_token(&ctx, q.token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    html_response(include_str!("static/index.html"))
}

/// GET /manifest.json
pub async fn get_manifest() -> Response {
    json_str_response(include_str!("static/manifest.json"))
}

/// GET /sw.js
pub async fn get_sw() -> Response {
    js_response(include_str!("static/sw.js"))
}

/// GET /app.js
pub async fn get_app_js() -> Response {
    js_response(include_str!("static/app.js"))
}

/// GET /api/health — liveness probe
pub async fn get_health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

// ── Status response types ──────────────────────────────────────────────────

#[derive(Serialize)]
pub struct StatusResponse {
    pub last_check_at: Option<String>,
    pub discovered_tools: Vec<String>,
    pub providers: std::collections::HashMap<String, crate::state::ProviderState>,
}

/// GET /api/status — return current AppState summary (token-gated)
pub async fn get_status(
    State(ctx): State<AppContext>,
    Query(q): Query<TokenQuery>,
) -> Response {
    if !check_token(&ctx, q.token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let state = ctx.state.lock().await;
    let resp = StatusResponse {
        last_check_at: state.last_check_at.clone(),
        discovered_tools: state.discovered_tools.clone(),
        providers: state.providers.clone(),
    };
    Json(resp).into_response()
}

// ── Subscribe ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SubscribeBody {
    pub endpoint: String,
    pub keys: SubscribeKeys,
}

#[derive(Deserialize)]
pub struct SubscribeKeys {
    pub p256dh: String,
    pub auth: String,
}

/// POST /api/subscribe — save a push subscription (deduplicated by endpoint)
pub async fn post_subscribe(
    State(ctx): State<AppContext>,
    Query(q): Query<TokenQuery>,
    Json(body): Json<SubscribeBody>,
) -> Response {
    if !check_token(&ctx, q.token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let mut state = ctx.state.lock().await;

    // Dedupe by endpoint
    let already_exists = state
        .push_subscriptions
        .iter()
        .any(|s| s.endpoint == body.endpoint);

    if !already_exists {
        state.push_subscriptions.push(PushSubscription {
            endpoint: body.endpoint,
            keys: PushKeys {
                p256dh: body.keys.p256dh,
                auth: body.keys.auth,
            },
            created_at: Utc::now().to_rfc3339(),
        });
    }

    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}
