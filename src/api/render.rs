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
    
    let data_keys: Vec<&str> = payload.data.as_object()
        .map(|obj| obj.keys().map(|k| k.as_str()).collect())
        .unwrap_or_default();
    
    tracing::info!(
        event = "render_html_started",
        template_size_bytes = template_size,
        data_keys = ?data_keys,
        "Starting HTML render"
    );

    let context = match tera::Context::from_value(payload.data.clone()) {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::error!(
                event = "render_html_error",
                stage = "context_creation",
                error = %e,
                data_keys = ?data_keys,
                "Failed to create Tera context from JSON data"
            );
            return Err(AppError::TemplateError(tera::Error::msg(format!("Invalid context data: {}", e))));
        }
    };
    
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
    
    let data_keys: Vec<&str> = payload.data.as_object()
        .map(|obj| obj.keys().map(|k| k.as_str()).collect())
        .unwrap_or_default();
    
    tracing::info!(
        event = "render_pdf_started",
        template_size_bytes = template_size,
        pdf_a = pdf_a_enabled,
        data_keys = ?data_keys,
        "Starting PDF render"
    );


    let context = match tera::Context::from_value(payload.data.clone()) {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::error!(
                event = "render_pdf_error",
                stage = "context_creation",
                error = %e,
                data_keys = ?data_keys,
                "Failed to create Tera context from JSON data"
            );
            return Err(AppError::TemplateError(tera::Error::msg(format!("Invalid context data: {}", e))));
        }
    };

    let html = match state.template_engine.render(&payload.template_html, &context) {
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

    // Browser rendering is fully blocking (CDP calls, thread::sleep). Running it
    // inline would stall the async runtime and freeze all in-flight requests,
    // so it must run on the blocking thread pool.
    let browser = state.browser.clone();
    let pdf_bytes = match tokio::task::spawn_blocking(move || browser.print_to_pdf(&html))
        .await
        .unwrap_or_else(|e| Err(anyhow::anyhow!("PDF render task failed: {}", e)))
    {
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

    let final_pdf = if pdf_a_enabled {
        // Ghostscript conversion blocks on the child process; same rule as the
        // browser stage — keep it off the async runtime.
        let original_size_bytes = pdf_bytes.len();
        match tokio::task::spawn_blocking(move || {
            crate::infra::ghostscript::Ghostscript::convert_to_pdfa(&pdf_bytes)
        })
        .await
        .unwrap_or_else(|e| Err(anyhow::anyhow!("PDF/A conversion task failed: {}", e)))
        {
            Ok(pdfa_bytes) => {
                tracing::debug!(
                    event = "pdfa_converted",
                    original_size_bytes = original_size_bytes,
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
