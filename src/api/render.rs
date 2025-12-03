use axum::{Json, response::IntoResponse, http::{StatusCode, header}, extract::State};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;
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
    let start = Instant::now();
    let template_size = payload.template_html.len();
    
    tracing::info!(
        event = "render_html_started",
        template_size_bytes = template_size,
        "Starting HTML render"
    );

    let context = tera::Context::from_value(payload.data).unwrap_or_default();
    
    match state.template_engine.render(&payload.template_html, &context) {
        Ok(html) => {
            let duration = start.elapsed();
            tracing::info!(
                event = "render_html_success",
                duration_ms = duration.as_millis() as u64,
                output_size_bytes = html.len(),
                "HTML render completed successfully"
            );
            Ok((StatusCode::OK, html))
        }
        Err(e) => {
            let duration = start.elapsed();
            tracing::error!(
                event = "render_html_error",
                duration_ms = duration.as_millis() as u64,
                error = %e,
                "HTML render failed"
            );
            Err(AppError::from(e))
        }
    }
}

pub async fn render_pdf(
    State(state): State<AppState>,
    Json(payload): Json<RenderRequest>,
) -> Result<impl IntoResponse, AppError> {
    let start = Instant::now();
    let template_size = payload.template_html.len();
    let pdf_a_enabled = payload.options.as_ref().map(|o| o.pdf_a).unwrap_or(false);
    
    tracing::info!(
        event = "render_pdf_started",
        template_size_bytes = template_size,
        pdf_a = pdf_a_enabled,
        "Starting PDF render"
    );

    // Renderizar template HTML
    let html = match state.template_engine.render(&payload.template_html, &tera::Context::from_value(payload.data).unwrap_or_default()) {
        Ok(html) => {
            tracing::debug!(
                event = "template_rendered",
                html_size_bytes = html.len(),
                "Template rendered to HTML"
            );
            html
        }
        Err(e) => {
            let duration = start.elapsed();
            tracing::error!(
                event = "render_pdf_error",
                stage = "template_rendering",
                duration_ms = duration.as_millis() as u64,
                error = %e,
                "PDF render failed at template stage"
            );
            return Err(AppError::from(e));
        }
    };

    // Gerar PDF via browser
    let pdf_bytes = match state.browser.print_to_pdf(&html) {
        Ok(bytes) => {
            tracing::debug!(
                event = "pdf_generated",
                pdf_size_bytes = bytes.len(),
                "PDF generated from HTML"
            );
            bytes
        }
        Err(e) => {
            let duration = start.elapsed();
            tracing::error!(
                event = "render_pdf_error",
                stage = "browser_pdf_generation",
                duration_ms = duration.as_millis() as u64,
                error = %e,
                "PDF render failed at browser stage"
            );
            return Err(AppError::BrowserError(e.to_string()));
        }
    };

    // Converter para PDF/A se necessário
    let final_pdf = if let Some(opts) = payload.options {
        if opts.pdf_a {
            match crate::infra::ghostscript::Ghostscript::convert_to_pdfa(&pdf_bytes) {
                Ok(pdfa_bytes) => {
                    tracing::debug!(
                        event = "pdfa_converted",
                        original_size_bytes = pdf_bytes.len(),
                        pdfa_size_bytes = pdfa_bytes.len(),
                        "PDF converted to PDF/A"
                    );
                    pdfa_bytes
                }
                Err(e) => {
                    let duration = start.elapsed();
                    tracing::error!(
                        event = "render_pdf_error",
                        stage = "pdfa_conversion",
                        duration_ms = duration.as_millis() as u64,
                        error = %e,
                        "PDF render failed at PDF/A conversion stage"
                    );
                    return Err(AppError::GhostscriptError(e.to_string()));
                }
            }
        } else {
            pdf_bytes
        }
    } else {
        pdf_bytes
    };

    let duration = start.elapsed();
    tracing::info!(
        event = "render_pdf_success",
        duration_ms = duration.as_millis() as u64,
        template_size_bytes = template_size,
        output_size_bytes = final_pdf.len(),
        pdf_a = pdf_a_enabled,
        "PDF render completed successfully"
    );

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        final_pdf
    ))
}
