use headless_chrome::{Browser, LaunchOptions};
use anyhow::Result;
use std::sync::{Arc, RwLock};
use std::time::Instant;

#[derive(Clone)]
pub struct BrowserManager {
    browser: Arc<RwLock<Browser>>,
}

impl BrowserManager {
    pub fn new() -> Result<Self> {
        tracing::info!(event = "browser_manager_init", "Initializing browser manager");
        let start = Instant::now();
        
        let browser = Self::create_browser()?;
        
        let duration = start.elapsed();
        tracing::info!(
            event = "browser_manager_ready",
            duration_ms = duration.as_millis() as u64,
            "Browser manager initialized and ready"
        );
        
        Ok(Self {
            browser: Arc::new(RwLock::new(browser)),
        })
    }

    fn create_browser() -> Result<Browser> {
        tracing::debug!(event = "browser_launching", "Launching headless Chrome browser");
        
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
        let start = std::time::Instant::now();
        let html_size = html.len();
        
        tracing::debug!(
            event = "browser_pdf_started",
            html_size_bytes = html_size,
            "Starting browser PDF generation"
        );

        let tab = {
            let browser_guard = self.browser.read().map_err(|_| anyhow::anyhow!("Browser lock poisoned"))?;
            browser_guard.new_tab()
        };

        let tab = match tab {
            Ok(t) => {
                tracing::debug!(event = "browser_tab_created", "Browser tab created successfully");
                t
            },
            Err(e) => {
                tracing::warn!(
                    event = "browser_tab_failed",
                    error = %e,
                    "Failed to create tab, attempting to restart browser"
                );
                let mut browser_guard = self.browser.write().map_err(|_| anyhow::anyhow!("Browser lock poisoned"))?;
                
                match Self::create_browser() {
                    Ok(new_browser) => {
                        *browser_guard = new_browser;
                        tracing::info!(event = "browser_restarted", "Browser restarted successfully");
                        browser_guard.new_tab()
                            .map_err(|e| anyhow::anyhow!("Failed to create tab after restart: {}", e))?
                    },
                    Err(e) => {
                        tracing::error!(
                            event = "browser_restart_failed",
                            error = %e,
                            "Failed to restart browser"
                        );
                        return Err(anyhow::anyhow!("Failed to restart browser: {}", e));
                    }
                }
            }
        };

        let html_data_url = format!("data:text/html;charset=utf-8,{}", urlencoding::encode(html));
        
        tab.navigate_to(&html_data_url)
            .map_err(|e| {
                tracing::error!(event = "browser_navigation_failed", error = %e, "Failed to navigate");
                anyhow::anyhow!("Failed to navigate: {}", e)
            })?
            .wait_until_navigated()
            .map_err(|e| {
                tracing::error!(event = "browser_wait_failed", error = %e, "Failed to wait for navigation");
                anyhow::anyhow!("Failed to wait for navigation: {}", e)
            })?;

        tracing::debug!(event = "browser_navigation_complete", "Navigation completed");

        let pdf_data = tab.print_to_pdf(None)
            .map_err(|e| {
                tracing::error!(event = "browser_print_failed", error = %e, "Failed to print to PDF");
                anyhow::anyhow!("Failed to print to PDF: {}", e)
            })?;

        let _ = tab.close(true);

        let duration = start.elapsed();
        tracing::debug!(
            event = "browser_pdf_complete",
            duration_ms = duration.as_millis() as u64,
            pdf_size_bytes = pdf_data.len(),
            "Browser PDF generation completed"
        );

        Ok(pdf_data)
    }
}
