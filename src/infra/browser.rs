use headless_chrome::{Browser, LaunchOptions};
use anyhow::Result;
use std::sync::Arc;

#[derive(Clone)]
pub struct BrowserManager {
    browser: Arc<Browser>,
}

impl BrowserManager {
    pub fn new() -> Result<Self> {
        let options = LaunchOptions::default_builder()
            .args(vec![
                std::ffi::OsStr::new("--no-sandbox"),
                std::ffi::OsStr::new("--disable-gpu"),
                std::ffi::OsStr::new("--disable-dev-shm-usage"),
            ])
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build launch options: {}", e))?;

        let browser = Browser::new(options)
            .map_err(|e| anyhow::anyhow!("Failed to launch browser: {}", e))?;

        Ok(Self {
            browser: Arc::new(browser),
        })
    }

    pub fn print_to_pdf(&self, html: &str) -> Result<Vec<u8>> {
        let tab = self.browser.new_tab()
            .map_err(|e| anyhow::anyhow!("Failed to create tab: {}", e))?;

        let html_data_url = format!("data:text/html;charset=utf-8,{}", urlencoding::encode(html));
        
        tab.navigate_to(&html_data_url)
            .map_err(|e| anyhow::anyhow!("Failed to navigate: {}", e))?
            .wait_until_navigated()
            .map_err(|e| anyhow::anyhow!("Failed to wait for navigation: {}", e))?;

        let pdf_data = tab.print_to_pdf(None)
            .map_err(|e| anyhow::anyhow!("Failed to print to PDF: {}", e))?;

        // Explicitly close the tab to prevent memory leaks (zombie tabs)
        // We ignore the error here because we already have the PDF data, 
        // and failing to close the tab shouldn't fail the request.
        let _ = tab.close(true);

        Ok(pdf_data)
    }
}
