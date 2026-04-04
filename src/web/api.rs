use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::config::Config;
use crate::state::{AppState, PushKeys, PushSubscription};

// ── Shared application context ─────────────────────────────────────────────

#[derive(Clone)]
pub struct AppContext {
    pub state: Arc<Mutex<AppState>>,
    pub access_token: Option<String>,
    pub config_path: PathBuf,
    pub vapid_public_key_path: PathBuf,
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

// ── VAPID public key ──────────────────────────────────────────────────────

/// GET /api/vapid-public-key — return the VAPID public key as base64url
///
/// The browser needs this as `applicationServerKey` for pushManager.subscribe().
/// We read the PEM, strip headers, decode the DER, and extract the 65-byte
/// uncompressed EC point (the last 65 bytes of the SubjectPublicKeyInfo).
pub async fn get_vapid_public_key(State(ctx): State<AppContext>) -> Response {
    let pem_bytes = match std::fs::read_to_string(&ctx.vapid_public_key_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "VAPID public key not found");
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "VAPID keys not generated. Run `psst init`." })),
            )
                .into_response();
        }
    };

    // Strip PEM headers and decode base64 to get the DER-encoded SubjectPublicKeyInfo.
    let b64: String = pem_bytes
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect();
    let der = match base64::engine::general_purpose::STANDARD.decode(&b64) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to decode VAPID public key PEM");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Invalid VAPID public key" })),
            )
                .into_response();
        }
    };

    // For a P-256 public key the DER SubjectPublicKeyInfo is 91 bytes:
    //   26 bytes header + 65 bytes uncompressed EC point (04 || x || y).
    // Extract the trailing 65 bytes.
    if der.len() < 65 {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "VAPID public key too short" })),
        )
            .into_response();
    }
    let raw_key = &der[der.len() - 65..];

    let encoded = URL_SAFE_NO_PAD.encode(raw_key);
    Json(serde_json::json!({ "publicKey": encoded })).into_response()
}

// ── Config API ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ConfigResponse {
    providers: std::collections::HashMap<String, ProviderConfigResponse>,
    check_interval_minutes: u32,
}

#[derive(Serialize, Deserialize, Clone)]
struct ProviderConfigResponse {
    monthly_fast_requests: Option<u64>,
    billing_day: Option<u32>,
    daily_token_limit: Option<u64>,
}

/// GET /api/config — return current provider configuration
pub async fn get_config(
    State(ctx): State<AppContext>,
    Query(q): Query<TokenQuery>,
) -> Response {
    if !check_token(&ctx, q.token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let config = match Config::load_from(&ctx.config_path) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to read config: {}", e) })),
            )
                .into_response();
        }
    };

    let providers = config
        .providers
        .into_iter()
        .map(|(id, pc)| {
            (
                id,
                ProviderConfigResponse {
                    monthly_fast_requests: pc.monthly_fast_requests,
                    billing_day: pc.billing_day,
                    daily_token_limit: pc.daily_token_limit,
                },
            )
        })
        .collect();

    Json(ConfigResponse {
        providers,
        check_interval_minutes: config.general.check_interval_minutes,
    })
    .into_response()
}

#[derive(Deserialize)]
pub struct UpdateConfigBody {
    providers: std::collections::HashMap<String, ProviderConfigResponse>,
}

/// POST /api/config — update provider limits and save to config.toml
pub async fn post_config(
    State(ctx): State<AppContext>,
    Query(q): Query<TokenQuery>,
    Json(body): Json<UpdateConfigBody>,
) -> Response {
    if !check_token(&ctx, q.token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    // Load current config, update providers, save back.
    let mut config = match Config::load_from(&ctx.config_path) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to read config: {}", e) })),
            )
                .into_response();
        }
    };

    for (id, update) in body.providers {
        let entry = config.providers.entry(id).or_default();
        entry.monthly_fast_requests = update.monthly_fast_requests;
        entry.billing_day = update.billing_day;
        entry.daily_token_limit = update.daily_token_limit;
    }

    // Serialize and save.
    let toml_str = match toml::to_string_pretty(&config) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to serialize config: {}", e) })),
            )
                .into_response();
        }
    };

    if let Err(e) = std::fs::write(&ctx.config_path, &toml_str) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to write config: {}", e) })),
        )
            .into_response();
    }

    tracing::info!("Config updated via dashboard");

    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}
