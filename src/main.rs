// SPDX-License-Identifier: AGPL-3.0-or-later
// VoidVeil — main.rs
// Drop-in AI privacy proxy. E2E encrypted. OpenAI-compatible.
//
// HTTP  → localhost:9999  (plain, for local tools)
// HTTPS → localhost:9998  (TLS, self-signed, install cert once)
//
// export OPENAI_BASE_URL=https://localhost:9998/v1
// export VOIDVEIL_UPSTREAM=https://api.anthropic.com
// voidveil
//
// Provider sees: tokens. Map: RAM only. Telemetry: stripped. You: invisible.

mod anonymise;
mod proxy;
mod tls;
mod token_map;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use token_map::TokenMap;
use proxy::Proxy;

#[derive(Clone)]
struct AppState {
    map: TokenMap,
    proxy: Proxy,
    session_id: String,
}

#[tokio::main]
async fn main() {
    // Install ring as the default rustls crypto provider
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("crypto provider");

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "voidveil=info".to_string())
        )
        .init();

    let http_port: u16  = std::env::var("VOIDVEIL_PORT").ok()
        .and_then(|p| p.parse().ok()).unwrap_or(9999);
    let https_port: u16 = std::env::var("VOIDVEIL_TLS_PORT").ok()
        .and_then(|p| p.parse().ok()).unwrap_or(9998);

    let upstream   = std::env::var("VOIDVEIL_UPSTREAM").ok();
    let session_id = TokenMap::session_id();

    let state = Arc::new(AppState {
        map: TokenMap::new(),
        proxy: Proxy::new(upstream),
        session_id: session_id.clone(),
    });

    let app = build_router(state.clone());

    // ── E2E TLS — HTTPS listener ─────────────────────────────────────────────
    let cert = tls::ensure_cert();
    let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem(
        cert.cert_pem.as_bytes().to_vec(),
        cert.key_pem.as_bytes().to_vec(),
    ).await.unwrap();

    let https_addr = SocketAddr::from(([127, 0, 0, 1], https_port));
    let http_addr  = SocketAddr::from(([127, 0, 0, 1], http_port));

    tracing::info!("VoidVeil HTTPS → {https_addr}");
    tracing::info!("VoidVeil HTTP  → {http_addr}  (loopback only)");
    tracing::info!("Session: {session_id}");
    tracing::info!("Cert: {}", cert.cert_path.display());
    tracing::info!("Telemetry: stripped | Map: RAM only | Sovereignty: complete");

    // Spawn HTTP (plain loopback) + HTTPS (encrypted) simultaneously
    let http_app = app.clone();
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(http_addr).await.unwrap();
        axum::serve(listener, http_app).await.unwrap();
    });

    axum_server::bind_rustls(https_addr, tls_config)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(handler))
        .route("/v1/messages",         post(handler))
        .route("/v1/completions",      post(handler))
        .route("/health",              get(health))
        .route("/v1/models",           get(models))
        .with_state(state)
}

// ── Universal handler — routes streaming vs non-streaming ────────────────────

async fn handler(
    State(s): State<Arc<AppState>>,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let path  = uri.path().to_string();
    let hdrs  = extract_headers(&headers);
    let is_stream = body.get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_stream {
        match s.proxy.forward_stream(&path, body, hdrs, &s.map).await {
            Ok(chunks) => {
                // Reassemble as SSE-compatible JSON response
                let full = chunks.join("\n");
                (StatusCode::OK, Json(json!({ "streamed_chunks": chunks.len(), "data": full }))).into_response()
            }
            Err(e) => {
                tracing::error!("stream error: {e}");
                (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))).into_response()
            }
        }
    } else {
        match s.proxy.forward(&path, body, hdrs, &s.map).await {
            Ok(r)  => (StatusCode::OK, Json(r)).into_response(),
            Err(e) => {
                tracing::error!("proxy error: {e}");
                (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))).into_response()
            }
        }
    }
}

fn extract_headers(headers: &HeaderMap) -> HashMap<String, String> {
    let passthrough = ["authorization", "x-api-key", "anthropic-version",
                       "anthropic-beta", "content-type"];
    let mut out = HashMap::new();
    for key in &passthrough {
        if let Some(v) = headers.get(*key) {
            if let Ok(s) = v.to_str() {
                out.insert(key.to_string(), s.to_string());
            }
        }
    }
    out.insert("content-type".to_string(), "application/json".to_string());
    out
}

async fn health(State(s): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "session": s.session_id,
        "tls": "active",
        "telemetry_stripped": true,
        "map": "RAM only — never transmitted",
        "sovereignty": "complete"
    }))
}

async fn models() -> Json<Value> {
    Json(json!({
        "object": "list",
        "data": [
            {"id": "voidveil-proxy",    "object": "model", "owned_by": "voidveil"},
            {"id": "claude-sonnet-4-6", "object": "model", "owned_by": "anthropic"},
            {"id": "claude-opus-4-6",   "object": "model", "owned_by": "anthropic"},
            {"id": "gpt-4o",            "object": "model", "owned_by": "openai"}
        ]
    }))
}
