use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Template error: {0}")]
    TemplateError(#[from] tera::Error),
    #[error("Browser error: {0}")]
    BrowserError(String),
    #[error("Ghostscript error: {0}")]
    GhostscriptError(String),
    #[error("Internal error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message, error_type) = match &self {
            AppError::TemplateError(e) => (StatusCode::BAD_REQUEST, e.to_string(), "template_error"),
            AppError::BrowserError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.clone(), "browser_error"),
            AppError::GhostscriptError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.clone(), "ghostscript_error"),
            AppError::Anyhow(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), "internal_error"),
        };

        tracing::error!(
            event = "api_error_response",
            error_type = error_type,
            status_code = status.as_u16(),
            error = %error_message,
            "API error response"
        );

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}
