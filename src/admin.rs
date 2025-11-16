use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Response, StatusCode},
    routing::{get, post},
};
use serde_json::json;
use tracing::info;

use crate::response::json_response;
use crate::settings::{Settings, SettingsLayer};
use crate::state::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/update", post(update))
        .route("/api/v1/reset", post(reset))
        .route("/api/v1/list", get(list_settings))
        .route("/api/v1/one-off", post(add_one_off))
        .route("/api/v1/list-headers", post(list_headers))
        .route("/", get(service_root))
        .route("/health", get(health))
        .route("/healthcheck", get(health))
        .fallback(not_found)
        .with_state(state)
}

async fn update(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response<Body> {
    let layer = SettingsLayer::from_headers(&headers);
    let snapshot = state.merge_admin(layer);
    json_response(StatusCode::OK, &snapshot, state.body_trailer())
}

async fn reset(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response<Body> {
    let layer = SettingsLayer::from_headers(&headers);
    let snapshot = state.reset_admin(layer);
    json_response(StatusCode::OK, &snapshot, state.body_trailer())
}

async fn list_settings(State(state): State<Arc<AppState>>) -> Response<Body> {
    let snapshot = state.admin_snapshot();
    json_response(StatusCode::OK, &snapshot, state.body_trailer())
}

async fn add_one_off(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response<Body> {
    let layer = SettingsLayer::from_headers(&headers);
    let mut settings = Settings::default();
    settings.apply_layer(&layer);
    state.add_one_off(settings);
    json_response(
        StatusCode::OK,
        &json!({"service":"lowdown","message":"Added one-off"}),
        state.body_trailer(),
    )
}

async fn list_headers(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response<Body> {
    let mut header_names: Vec<String> = headers
        .keys()
        .map(|name| name.as_str().to_string())
        .collect();
    header_names.sort();
    for name in &header_names {
        if name.to_ascii_lowercase().starts_with("x-lowdown-") {
            if let Some(value) = headers.get(name) {
                info!("x-lowdown- Header {name} => {:?}", value);
            }
        }
    }
    for name in &header_names {
        if !name.to_ascii_lowercase().starts_with("x-lowdown-") {
            if let Some(value) = headers.get(name) {
                info!("Other header {name} => {:?}", value);
            }
        }
    }
    json_response(StatusCode::OK, &json!(header_names), state.body_trailer())
}

async fn service_root(State(state): State<Arc<AppState>>) -> Response<Body> {
    json_response(
        StatusCode::OK,
        &json!({"service":"lowdown"}),
        state.body_trailer(),
    )
}

async fn health(State(state): State<Arc<AppState>>) -> Response<Body> {
    json_response(
        StatusCode::OK,
        &json!({"service":"lowdown","status":"healthy"}),
        state.body_trailer(),
    )
}

async fn not_found(State(state): State<Arc<AppState>>) -> Response<Body> {
    json_response(
        StatusCode::NOT_FOUND,
        &json!({"message":"not-found"}),
        state.body_trailer(),
    )
}
