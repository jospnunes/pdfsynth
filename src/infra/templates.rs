use tera::Tera;
use std::time::Instant;

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
        
        tracing::debug!(
            event = "template_render_started",
            template_size_bytes = template_size,
            "Starting template rendering"
        );

        match Tera::one_off(template_str, context, true) {
            Ok(result) => {
                let duration = start.elapsed();
                tracing::debug!(
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
                tracing::error!(
                    event = "template_render_failed",
                    duration_ms = duration.as_millis() as u64,
                    error = %e,
                    "Template rendering failed"
                );
                Err(e)
            }
        }
    }
}
