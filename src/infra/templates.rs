use tera::Tera;
use std::time::Instant;
use std::error::Error;

#[derive(Clone)]
pub struct TemplateEngine;

impl TemplateEngine {
    pub fn new() -> anyhow::Result<Self> {
        tracing::debug!(event = "template_engine_created", "Template engine instance created");
        Ok(Self)
    }

    pub fn render(&self, template_str: &str, context: &tera::Context) -> std::result::Result<String, tera::Error> {
        let start = Instant::now();
        let template_size = template_str.len();
        
        // Extrair chaves do contexto para log
        let context_json = context.clone().into_json();
        let context_keys: Vec<&str> = context_json.as_object()
            .map(|obj| obj.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();
        
        tracing::info!(
            event = "template_render_started",
            template_size_bytes = template_size,
            context_keys = ?context_keys,
            "Starting template rendering"
        );

        // Log de amostra do template (primeiros 500 chars)
        let template_preview: String = template_str.chars().take(500).collect();
        tracing::debug!(
            event = "template_preview",
            preview = %template_preview,
            "Template content preview"
        );

        match Tera::one_off(template_str, context, false) {
            Ok(result) => {
                let duration = start.elapsed();
                tracing::info!(
                    event = "template_render_success",
                    duration_ms = duration.as_millis() as u64,
                    template_size_bytes = template_size,
                    output_size_bytes = result.len(),
                    "Template rendered successfully"
                );
                Ok(result)
            }
            Err(e) => {
                let duration = start.elapsed();
                
                // Extrair detalhes do erro Tera
                let error_source = e.source().map(|s| s.to_string()).unwrap_or_default();
                
                // Tentar identificar a linha/variável problemática
                let error_details = format!("{:#}", e);
                
                tracing::error!(
                    event = "template_render_failed",
                    duration_ms = duration.as_millis() as u64,
                    error = %e,
                    error_details = %error_details,
                    error_source = %error_source,
                    context_keys = ?context_keys,
                    template_size_bytes = template_size,
                    "Template rendering failed - check if all variables in template exist in context"
                );
                
                // Log adicional com o contexto completo em debug
                tracing::debug!(
                    event = "template_render_failed_context",
                    context = %context_json,
                    "Full context data for failed render"
                );
                
                Err(e)
            }
        }
    }
}
