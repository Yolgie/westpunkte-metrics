use std::sync::Arc;

use axum::{Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use chrono::Utc;
use tokio::sync::RwLock;

use crate::metrics::{MetricsSnapshot, render};

pub type SharedSnapshot = Arc<RwLock<MetricsSnapshot>>;

const INDEX_BODY: &str = "westpunkte-metrics exporter\n\nEndpoints:\n  GET /metrics  Prometheus metrics\n  GET /health   Liveness probe\n";

pub fn router(state: SharedSnapshot) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
}

async fn index() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        INDEX_BODY,
    )
}

async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        "ok\n",
    )
}

async fn metrics_handler(State(state): State<SharedSnapshot>) -> impl IntoResponse {
    let snapshot = state.read().await.clone();
    let body = render(&snapshot, Utc::now());
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}
