use std::process::Command;
use std::io::Write;
use anyhow::Result;
use tempfile::NamedTempFile;

pub struct Ghostscript;

impl Ghostscript {
    pub fn convert_to_pdfa(pdf_data: &[u8]) -> Result<Vec<u8>> {
        let mut input_file = NamedTempFile::new()?;
        input_file.write_all(pdf_data)?;
        
        let output_file = NamedTempFile::new()?;
        let output_path = output_file.path().to_str().unwrap().to_string();

        let status = Command::new("gs")
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
            .status()
            .map_err(|e| anyhow::anyhow!("Failed to execute ghostscript: {}", e))?;

        if !status.success() {
            return Err(anyhow::anyhow!("Ghostscript failed with status: {}", status));
        }

        let output_data = std::fs::read(output_path)?;
        Ok(output_data)
    }
}
