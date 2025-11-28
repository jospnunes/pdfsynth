use headless_chrome::{Browser, LaunchOptions};
use anyhow::Result;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct BrowserManager {
    browser: Arc<RwLock<Browser>>,
}

impl BrowserManager {
    pub fn new() -> Result<Self> {
        let browser = Self::create_browser()?;
        Ok(Self {
            browser: Arc::new(RwLock::new(browser)),
        })
    }

    fn create_browser() -> Result<Browser> {
        let options = LaunchOptions::default_builder()
            .args(vec![
                std::ffi::OsStr::new("--no-sandbox"),
                std::ffi::OsStr::new("--disable-gpu"),
                std::ffi::OsStr::new("--disable-dev-shm-usage"),
                std::ffi::OsStr::new("--disable-software-rasterizer"),
                std::ffi::OsStr::new("--disable-extensions"),
            ])
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build launch options: {}", e))?;

        Browser::new(options)
            .map_err(|e| anyhow::anyhow!("Failed to launch browser: {}", e))
    }

    pub fn print_to_pdf(&self, html: &str) -> Result<Vec<u8>> {
        let tab = {
            let browser_guard = self.browser.read().map_err(|_| anyhow::anyhow!("Browser lock poisoned"))?;
            browser_guard.new_tab()
        };

        let tab = match tab {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("Failed to create tab, attempting to restart browser: {}", e);
                let mut browser_guard = self.browser.write().map_err(|_| anyhow::anyhow!("Browser lock poisoned"))?;
                
                match Self::create_browser() {
                    Ok(new_browser) => {
                        *browser_guard = new_browser;
                        tracing::info!("Browser restarted successfully");
                        browser_guard.new_tab()
                            .map_err(|e| anyhow::anyhow!("Failed to create tab after restart: {}", e))?
                    },
                    Err(e) => {
                        return Err(anyhow::anyhow!("Failed to restart browser: {}", e));
                    }
                }
            }
        };

        let html_data_url = format!("data:text/html;charset=utf-8,{}", urlencoding::encode(html));
        
        tab.navigate_to(&html_data_url)
            .map_err(|e| anyhow::anyhow!("Failed to navigate: {}", e))?
            .wait_until_navigated()
            .map_err(|e| anyhow::anyhow!("Failed to wait for navigation: {}", e))?;

        let pdf_data = tab.print_to_pdf(None)
            .map_err(|e| anyhow::anyhow!("Failed to print to PDF: {}", e))?;

        let _ = tab.close(true);

        Ok(pdf_data)
    }
}
