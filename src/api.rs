use crate::peer::PeerMap;
use axum::{
    extract::{Extension, Path},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Router,
};
use hbb_common::log;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Clone)]
struct ApiState {
    pm: PeerMap,
    api_key: String,
}

fn check_auth(headers: &HeaderMap, api_key: &str) -> bool {
    if api_key.is_empty() {
        return true;
    }
    headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == api_key)
        .unwrap_or(false)
}

// ── Response types ─────────────────────────────────────────────────────────

#[derive(Serialize)]
struct PeerResponse {
    id: String,
    online: bool,
    disabled: bool,
    last_online: Option<String>,
    created_at: Option<String>,
    ip: String,
    note: Option<String>,
}

#[derive(Serialize)]
struct WhitelistResponse {
    id: String,
    note: Option<String>,
    created_at: Option<String>,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

#[derive(Deserialize)]
struct AddWhitelistBody {
    id: String,
    #[serde(default)]
    note: String,
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn unauthorized() -> impl IntoResponse {
    (
        StatusCode::UNAUTHORIZED,
        Json(ApiError {
            error: "Unauthorized".into(),
        }),
    )
}

fn bad_request(msg: impl Into<String>) -> impl IntoResponse {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError { error: msg.into() }),
    )
}

fn internal_error(msg: impl Into<String>) -> impl IntoResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError { error: msg.into() }),
    )
}

// ── Handlers ───────────────────────────────────────────────────────────────

async fn list_peers(
    Extension(state): Extension<ApiState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !check_auth(&headers, &state.api_key) {
        return unauthorized().into_response();
    }
    let records = match state.pm.db.list_peers().await {
        Ok(r) => r,
        Err(e) => return internal_error(e.to_string()).into_response(),
    };
    let online_set = state.pm.get_online_set().await;
    let peers: Vec<PeerResponse> = records
        .into_iter()
        .map(|r| {
            let online = online_set.contains(&r.id);
            let ip = serde_json::from_str::<serde_json::Value>(&r.info)
                .ok()
                .and_then(|v| v.get("ip").and_then(|ip| ip.as_str()).map(String::from))
                .unwrap_or_default();
            PeerResponse {
                disabled: r.status == Some(0),
                id: r.id,
                online,
                last_online: r.last_online,
                created_at: r.created_at,
                ip,
                note: r.note,
            }
        })
        .collect();
    Json(peers).into_response()
}

async fn disable_peer(
    Extension(state): Extension<ApiState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !check_auth(&headers, &state.api_key) {
        return unauthorized().into_response();
    }
    if let Err(e) = state.pm.db.set_peer_status(&id, 0).await {
        return internal_error(e.to_string()).into_response();
    }
    state.pm.evict(&id).await;
    log::info!("Peer {} disabled via API", id);
    Json(serde_json::json!({"ok": true})).into_response()
}

async fn enable_peer(
    Extension(state): Extension<ApiState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !check_auth(&headers, &state.api_key) {
        return unauthorized().into_response();
    }
    if let Err(e) = state.pm.db.set_peer_status(&id, 1).await {
        return internal_error(e.to_string()).into_response();
    }
    state.pm.evict(&id).await;
    log::info!("Peer {} enabled via API", id);
    Json(serde_json::json!({"ok": true})).into_response()
}

async fn list_whitelist(
    Extension(state): Extension<ApiState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !check_auth(&headers, &state.api_key) {
        return unauthorized().into_response();
    }
    match state.pm.db.list_whitelist().await {
        Ok(entries) => {
            let resp: Vec<WhitelistResponse> = entries
                .into_iter()
                .map(|e| WhitelistResponse {
                    id: e.id,
                    note: e.note,
                    created_at: e.created_at,
                })
                .collect();
            Json(resp).into_response()
        }
        Err(e) => internal_error(e.to_string()).into_response(),
    }
}

async fn add_whitelist(
    Extension(state): Extension<ApiState>,
    headers: HeaderMap,
    Json(body): Json<AddWhitelistBody>,
) -> impl IntoResponse {
    if !check_auth(&headers, &state.api_key) {
        return unauthorized().into_response();
    }
    if body.id.len() < 6 {
        return bad_request("ID must be at least 6 characters").into_response();
    }
    if let Err(e) = state.pm.db.add_to_whitelist(&body.id, &body.note).await {
        return internal_error(e.to_string()).into_response();
    }
    log::info!("Whitelist: added {}", body.id);
    Json(serde_json::json!({"ok": true})).into_response()
}

async fn remove_whitelist(
    Extension(state): Extension<ApiState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !check_auth(&headers, &state.api_key) {
        return unauthorized().into_response();
    }
    if let Err(e) = state.pm.db.remove_from_whitelist(&id).await {
        return internal_error(e.to_string()).into_response();
    }
    log::info!("Whitelist: removed {}", id);
    Json(serde_json::json!({"ok": true})).into_response()
}

// ── Server startup ─────────────────────────────────────────────────────────

pub(crate) async fn start(pm: PeerMap, port: u16) {
    let api_key = std::env::var("API_KEY").unwrap_or_default();
    if api_key.is_empty() {
        log::warn!("API_KEY is not set — management API is open to anyone");
    }
    let state = ApiState { pm, api_key };

    let app = Router::new()
        .route("/api/peers", get(list_peers))
        .route("/api/peers/:id/disable", post(disable_peer))
        .route("/api/peers/:id/enable", post(enable_peer))
        .route("/api/whitelist", get(list_whitelist))
        .route("/api/whitelist", post(add_whitelist))
        .route("/api/whitelist/:id", delete(remove_whitelist))
        .layer(Extension(state));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    log::info!("Management API listening on :{}", port);
    if let Err(e) = axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
    {
        log::error!("Management API server error: {}", e);
    }
}
