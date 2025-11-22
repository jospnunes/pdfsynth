use axum::{Json, response::IntoResponse, http::{StatusCode, header}, extract::State};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::api::state::AppState;
use crate::api::error::AppError;

#[derive(Deserialize, Serialize, Debug)]
pub struct RenderOptions {
    pub pdf_a: bool,
    pub paper_format: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct RenderRequest {
    pub template_html: String,
    pub data: Value,
    pub options: Option<RenderOptions>,
}

pub async fn render_html(
    State(state): State<AppState>,
    Json(payload): Json<RenderRequest>,
) -> Result<impl IntoResponse, AppError> {
    let context = tera::Context::from_value(payload.data).unwrap_or_default();
    let html = state.template_engine.render(&payload.template_html, &context)?;
    Ok((StatusCode::OK, html))
}

pub async fn render_pdf(
    State(state): State<AppState>,
    Json(payload): Json<RenderRequest>,
) -> Result<impl IntoResponse, AppError> {
    let context = tera::Context::from_value(payload.data).unwrap_or_default();
    let html = state.template_engine.render(&payload.template_html, &context)?;

    let pdf_bytes = state.browser.print_to_pdf(&html)
        .map_err(|e| AppError::BrowserError(e.to_string()))?;

    let final_pdf = if let Some(opts) = payload.options {
        if opts.pdf_a {
            crate::infra::ghostscript::Ghostscript::convert_to_pdfa(&pdf_bytes)
                .map_err(|e| AppError::GhostscriptError(e.to_string()))?
        } else {
            pdf_bytes
        }
    } else {
        pdf_bytes
    };

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        final_pdf
    ))
}
