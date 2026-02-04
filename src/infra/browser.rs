use headless_chrome::{Browser, LaunchOptions};
use headless_chrome::types::PrintToPdfOptions;
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
                std::ffi::OsStr::new("--allow-file-access-from-files"),
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

        tab.navigate_to("about:blank")
            .map_err(|e| {
                tracing::error!(event = "browser_navigation_failed", error = %e, "Failed to navigate to blank");
                anyhow::anyhow!("Failed to navigate to blank: {}", e)
            })?
            .wait_until_navigated()
            .map_err(|e| {
                tracing::error!(event = "browser_wait_failed", error = %e, "Failed to wait for blank navigation");
                anyhow::anyhow!("Failed to wait for blank navigation: {}", e)
            })?;

        tracing::debug!(
            event = "browser_setting_content",
            html_size_bytes = html_size,
            "Setting document content directly"
        );

        let set_content_script = format!(
            r#"document.open(); document.write({}); document.close();"#,
            serde_json::to_string(html).unwrap_or_else(|_| "''".to_string())
        );

        tab.evaluate(&set_content_script, false)
            .map_err(|e| {
                tracing::error!(event = "browser_set_content_failed", error = %e, "Failed to set document content");
                anyhow::anyhow!("Failed to set document content: {}", e)
            })?;

        std::thread::sleep(std::time::Duration::from_millis(300));

        tracing::debug!(event = "browser_navigation_complete", "Navigation completed");

        let wait_for_images_script = r#"
            new Promise((resolve) => {
                const imgTags = Array.from(document.querySelectorAll('img'));
                
                const bgImages = [];
                document.querySelectorAll('*').forEach(el => {
                    const bg = getComputedStyle(el).backgroundImage;
                    if (bg && bg !== 'none') {
                        const urlMatch = bg.match(/url\(["']?([^"')]+)["']?\)/);
                        if (urlMatch && urlMatch[1]) {
                            bgImages.push(urlMatch[1]);
                        }
                    }
                });
                
                let pending = imgTags.length + bgImages.length;
                
                if (pending === 0) {
                    setTimeout(() => resolve('no_images'), 300);
                    return;
                }
                
                const checkComplete = () => {
                    pending--;
                    if (pending <= 0) {
                        setTimeout(() => resolve('all_loaded'), 500);
                    }
                };
                
                imgTags.forEach(img => {
                    if (img.complete && img.naturalHeight !== 0) {
                        checkComplete();
                    } else {
                        img.onload = checkComplete;
                        img.onerror = checkComplete;
                    }
                });
                
                bgImages.forEach(url => {
                    const img = new Image();
                    img.onload = checkComplete;
                    img.onerror = checkComplete;
                    img.src = url;
                });
                
                setTimeout(() => resolve('timeout'), 15000);
            })
        "#;

        tracing::debug!(event = "browser_waiting_images", "Waiting for images to load");
        
        let wait_result = tab.evaluate(wait_for_images_script, true)
            .map_err(|e| {
                tracing::warn!(event = "browser_wait_images_failed", error = %e, "Failed to wait for images");
                e
            });

        if let Ok(result) = wait_result {
            tracing::debug!(
                event = "browser_images_loaded",
                result = ?result.value,
                "Images loading completed"
            );
        }

        let pdf_data = tab.print_to_pdf(Some(PrintToPdfOptions {
            print_background: Some(true),
            prefer_css_page_size: Some(true),
            margin_top: Some(0.0),
            margin_bottom: Some(0.0),
            margin_left: Some(0.0),
            margin_right: Some(0.0),
            ..Default::default()
        }))
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
