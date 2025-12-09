use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

pub async fn health_check() -> impl IntoResponse {
    tracing::debug!(event = "health_check", status = "ok", "Health check requested");
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}
