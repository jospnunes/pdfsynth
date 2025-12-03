use std::process::Command;
use std::io::Write;
use std::time::Instant;
use anyhow::Result;
use tempfile::NamedTempFile;

pub struct Ghostscript;

impl Ghostscript {
    pub fn convert_to_pdfa(pdf_data: &[u8]) -> Result<Vec<u8>> {
        let start = Instant::now();
        let input_size = pdf_data.len();
        
        tracing::debug!(
            event = "ghostscript_pdfa_started",
            input_size_bytes = input_size,
            "Starting PDF/A conversion with Ghostscript"
        );

        let mut input_file = NamedTempFile::new()?;
        input_file.write_all(pdf_data)?;
        
        let output_file = NamedTempFile::new()?;
        let output_path = output_file.path().to_str().unwrap().to_string();

        let output = Command::new("gs")
            .arg("-dPDFA=1")
            .arg("-dBATCH")
            .arg("-dNOPAUSE")
            .arg("-dNOOUTERSAVE")
            .arg("-sColorConversionStrategy=RGB")
            .arg("-sProcessColorModel=DeviceRGB")
            .arg("-sDEVICE=pdfwrite")
            .arg("-dPDFACompatibilityPolicy=1")
            .arg(format!("-sOutputFile={}", output_path))
            .arg("assets/PDFA_def.ps")
            .arg(input_file.path())
            .output()
            .map_err(|e| {
                tracing::error!(
                    event = "ghostscript_execute_failed",
                    error = %e,
                    "Failed to execute Ghostscript"
                );
                anyhow::anyhow!("Failed to execute ghostscript: {}", e)
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let duration = start.elapsed();
            tracing::error!(
                event = "ghostscript_pdfa_failed",
                duration_ms = duration.as_millis() as u64,
                exit_code = output.status.code(),
                stderr = %stderr,
                "Ghostscript PDF/A conversion failed"
            );
            return Err(anyhow::anyhow!("Ghostscript failed with status: {}. Stderr: {}", output.status, stderr));
        }

        let output_data = std::fs::read(&output_path)?;
        let duration = start.elapsed();
        
        tracing::debug!(
            event = "ghostscript_pdfa_complete",
            duration_ms = duration.as_millis() as u64,
            input_size_bytes = input_size,
            output_size_bytes = output_data.len(),
            "PDF/A conversion completed successfully"
        );

        Ok(output_data)
    }
}
