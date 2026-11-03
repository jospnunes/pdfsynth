use headless_chrome::{Browser, LaunchOptions, Tab};
use headless_chrome::browser::tab::{RequestPausedDecision, RequestInterceptor};
use headless_chrome::types::PrintToPdfOptions;
use headless_chrome::protocol::cdp::{Emulation, Fetch, Network, Page};
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Timeout for every CDP call and for the load event, only in secure mode.
const SECURE_LOAD_TIMEOUT: Duration = Duration::from_secs(15);

/// Secure mode: only these URL schemes may be requested by the rendered document.
/// Everything else (http, https, file, ws, ftp, chrome, blob, filesystem...) is failed.
/// Number of requests failed by the interceptor (only used by tests).
#[cfg(test)]
pub(crate) static BLOCKED_REQUESTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn is_allowed_url(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    lower.starts_with("data:") || lower.starts_with("about:")
}

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

        let chrome_path = ["/usr/bin/chromium", "/usr/bin/chromium-browser", "/usr/bin/google-chrome"]
            .iter()
            .find(|p| std::path::Path::new(p).exists())
            .map(|p| std::path::PathBuf::from(*p));

        if let Some(ref path) = chrome_path {
            tracing::info!(event = "browser_using_system_chrome", path = %path.display(), "Using system Chrome binary");
        }

        let mut builder = LaunchOptions::default_builder();
        builder
            .args(vec![
                std::ffi::OsStr::new("--no-sandbox"),
                std::ffi::OsStr::new("--disable-gpu"),
                std::ffi::OsStr::new("--disable-dev-shm-usage"),
                std::ffi::OsStr::new("--disable-software-rasterizer"),
                std::ffi::OsStr::new("--disable-extensions"),
                std::ffi::OsStr::new("--allow-file-access-from-files"),
            ]);

        if let Some(path) = chrome_path {
            builder.path(Some(path));
        }

        let options = builder
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build launch options: {}", e))?;

        Browser::new(options)
            .map_err(|e| anyhow::anyhow!("Failed to launch browser: {}", e))
    }

    /// `secure = false`: legacy behavior (JavaScript on, network and external
    /// resources allowed). `secure = true`: JavaScript off and only `data:` /
    /// `about:` requests allowed (see `harden_tab`).
    pub fn print_to_pdf(&self, html: &str, secure: bool) -> Result<Vec<u8>> {
        let start = std::time::Instant::now();
        let html_size = html.len();
        
        tracing::debug!(
            event = "browser_pdf_started",
            html_size_bytes = html_size,
            secure = secure,
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

        // Always close the tab, also when rendering fails.
        let result = if secure {
            Self::render_secure(&tab, html)
        } else {
            Self::render_legacy(&tab, html)
        };
        let _ = tab.close(true);
        let pdf_data = result?;


        let duration = start.elapsed();
        tracing::debug!(
            event = "browser_pdf_complete",
            duration_ms = duration.as_millis() as u64,
            pdf_size_bytes = pdf_data.len(),
            "Browser PDF generation completed"
        );

        Ok(pdf_data)
    }

    fn render_legacy(tab: &Arc<Tab>, html: &str) -> Result<Vec<u8>> {
        let html_size = html.len();

        tracing::debug!(
            event = "browser_setting_content",
            html_size_bytes = html_size,
            "Setting document content via CDP"
        );

        tab.navigate_to("about:blank")
            .map_err(|e| anyhow::anyhow!("Failed to navigate to blank: {}", e))?
            .wait_until_navigated()
            .map_err(|e| anyhow::anyhow!("Failed to wait for blank navigation: {}", e))?;

        let frame_tree = tab.call_method(Page::GetFrameTree(None))
            .map_err(|e| anyhow::anyhow!("Failed to get frame tree: {}", e))?;
        let frame_id = frame_tree.frame_tree.frame.id;

        tab.call_method(Page::SetDocumentContent {
            frame_id,
            html: html.to_string(),
        })
        .map_err(|e| anyhow::anyhow!("Failed to set document content: {}", e))?;

        std::thread::sleep(std::time::Duration::from_millis(300));

        tracing::debug!(event = "browser_content_set", "Document content set successfully");

        let wait_for_images_script = r#"
            new Promise((resolve) => {
                const imgTags = Array.from(document.querySelectorAll('img'));
                
                const bgImages = [];
                document.querySelectorAll('*').forEach(el => {
                    const bg = getComputedStyle(el).backgroundImage;
                    if (bg && bg !== 'none') {
                        const urlMatch = bg.match(/url\(["']?(data:[^"')]+|[^"')]+)["']?\)/);
                        if (urlMatch && urlMatch[1]) {
                            bgImages.push({ url: urlMatch[1], element: el });
                        }
                    }
                });
                
                let pending = imgTags.length + bgImages.length;
                
                console.log('Images to load:', { imgTags: imgTags.length, bgImages: bgImages.length });
                
                if (pending === 0) {
                    setTimeout(() => resolve('no_images'), 300);
                    return;
                }
                
                const checkComplete = () => {
                    pending--;
                    if (pending <= 0) {
                        setTimeout(() => resolve('all_loaded'), 1000);
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
                
                bgImages.forEach(({ url }) => {
                    if (url.startsWith('data:')) {
                        const img = new Image();
                        img.onload = checkComplete;
                        img.onerror = checkComplete;
                        img.src = url;
                    } else {
                        const img = new Image();
                        img.onload = checkComplete;
                        img.onerror = checkComplete;
                        img.src = url;
                    }
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

        tab.print_to_pdf(Some(PrintToPdfOptions {
            print_background: Some(true),
            prefer_css_page_size: Some(true),
            landscape: Some(true),
            paper_width: Some(11.69),   // A4 width in inches (297mm)
            paper_height: Some(8.27),   // A4 height in inches (210mm)
            scale: Some(1.0),
            margin_top: Some(0.0),
            margin_bottom: Some(0.0),
            margin_left: Some(0.0),
            margin_right: Some(0.0),
            ..Default::default()
        }))
            .map_err(|e| {
                tracing::error!(event = "browser_print_failed", error = %e, "Failed to print to PDF");
                anyhow::anyhow!("Failed to print to PDF: {}", e)
            })
    }

    /// Locks the tab down BEFORE any untrusted content is loaded:
    /// 1. JavaScript is disabled (`Emulation.setScriptExecutionDisabled`), so
    ///    `<script>`, inline handlers (`onerror`, `onload`...) and `javascript:` URLs never run;
    /// 2. every request is intercepted (`Fetch.enable` + request interceptor) and
    ///    only `data:` and `about:` URLs are allowed, the rest is failed.
    fn harden_tab(tab: &Arc<Tab>) -> Result<()> {
        tab.set_default_timeout(SECURE_LOAD_TIMEOUT);

        tab.call_method(Emulation::SetScriptExecutionDisabled { value: true })
            .map_err(|e| anyhow::anyhow!("Failed to disable JavaScript: {}", e))?;

        let interceptor: Arc<dyn RequestInterceptor + Send + Sync> = Arc::new(
            |_transport, _session_id, event: Fetch::events::RequestPausedEvent| {
                let url = event.params.request.url.as_str();
                if is_allowed_url(url) {
                    RequestPausedDecision::Continue(None)
                } else {
                    #[cfg(test)]
                    BLOCKED_REQUESTS.fetch_add(1, Ordering::SeqCst);
                    // Never log the full URL: it may carry data from the document.
                    tracing::warn!(
                        event = "browser_request_blocked",
                        scheme = url.split(':').next().unwrap_or(""),
                        "Blocked non-data request from rendered document"
                    );
                    RequestPausedDecision::Fail(Fetch::FailRequest {
                        request_id: event.params.request_id,
                        error_reason: Network::ErrorReason::BlockedByClient,
                    })
                }
            },
        );
        tab.enable_request_interception(interceptor)
            .map_err(|e| anyhow::anyhow!("Failed to set request interceptor: {}", e))?;
        // No patterns = intercept every request of the tab.
        tab.enable_fetch(None, None)
            .map_err(|e| anyhow::anyhow!("Failed to enable request interception: {}", e))?;

        Ok(())
    }

    fn render_secure(tab: &Arc<Tab>, html: &str) -> Result<Vec<u8>> {
        Self::harden_tab(tab)?;

        tracing::debug!(
            event = "browser_setting_content",
            html_size_bytes = html.len(),
            "Setting document content via CDP (secure mode)"
        );

        tab.navigate_to("about:blank")
            .map_err(|e| anyhow::anyhow!("Failed to navigate to blank: {}", e))?
            .wait_until_navigated()
            .map_err(|e| anyhow::anyhow!("Failed to wait for blank navigation: {}", e))?;

        // JavaScript is off, so the image-wait script of legacy mode cannot run:
        // wait for the load event of the new content instead (images are data: URIs).
        let loaded = Arc::new(AtomicBool::new(false));
        let loaded_flag = loaded.clone();
        let listener = tab
            .add_event_listener(Arc::new(move |event: &headless_chrome::protocol::cdp::types::Event| {
                if let headless_chrome::protocol::cdp::types::Event::PageLifecycleEvent(e) = event {
                    if e.params.name == "load" {
                        loaded_flag.store(true, Ordering::SeqCst);
                    }
                }
            }))
            .map_err(|e| anyhow::anyhow!("Failed to add event listener: {}", e))?;

        let frame_tree = tab.call_method(Page::GetFrameTree(None))
            .map_err(|e| anyhow::anyhow!("Failed to get frame tree: {}", e))?;
        let frame_id = frame_tree.frame_tree.frame.id;

        tab.call_method(Page::SetDocumentContent {
            frame_id,
            html: html.to_string(),
        })
        .map_err(|e| anyhow::anyhow!("Failed to set document content: {}", e))?;

        let deadline = Instant::now() + SECURE_LOAD_TIMEOUT;
        while !loaded.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
        let _ = tab.remove_event_listener(&listener);
        // Settle time for layout and image decoding.
        std::thread::sleep(Duration::from_millis(300));

        tracing::debug!(
            event = "browser_content_set",
            load_event_seen = loaded.load(Ordering::SeqCst),
            "Document content set"
        );

        Self::print(tab)
    }

    fn print(tab: &Arc<Tab>) -> Result<Vec<u8>> {
        tab.print_to_pdf(Some(PrintToPdfOptions {
            print_background: Some(true),
            prefer_css_page_size: Some(true),
            landscape: Some(true),
            paper_width: Some(11.69),   // A4 width in inches (297mm)
            paper_height: Some(8.27),   // A4 height in inches (210mm)
            scale: Some(1.0),
            margin_top: Some(0.0),
            margin_bottom: Some(0.0),
            margin_left: Some(0.0),
            margin_right: Some(0.0),
            ..Default::default()
        }))
        .map_err(|e| {
            tracing::error!(event = "browser_print_failed", error = %e, "Failed to print to PDF");
            anyhow::anyhow!("Failed to print to PDF: {}", e)
        })
    }
}

// These tests launch a real Chromium (the first of /usr/bin/chromium,
// /usr/bin/chromium-browser, /usr/bin/google-chrome). When none exists they are
// skipped (they print a notice and return). Text is extracted with `pdftotext`
// (poppler-utils); without it the tests are skipped as well.
// Run: cargo test -- --nocapture
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::AtomicUsize;
    use std::sync::OnceLock;

    // 1x1 red PNG.
    const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

    fn env_ok() -> bool {
        let chrome = ["/usr/bin/chromium", "/usr/bin/chromium-browser", "/usr/bin/google-chrome"]
            .iter()
            .any(|p| std::path::Path::new(p).exists());
        let pdftotext = std::process::Command::new("pdftotext").arg("-v").output().is_ok();
        if !(chrome && pdftotext) {
            eprintln!("SKIPPED: needs a Chrome/Chromium binary and pdftotext");
        }
        chrome && pdftotext
    }

    fn manager() -> BrowserManager {
        static M: OnceLock<BrowserManager> = OnceLock::new();
        M.get_or_init(|| BrowserManager::new().expect("browser")).clone()
    }

    fn text_of(pdf: &[u8]) -> String {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.pdf");
        std::fs::write(&path, pdf).unwrap();
        let out = std::process::Command::new("pdftotext").arg(&path).arg("-").output().unwrap();
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// Local HTTP server that counts requests and answers a PNG.
    fn spawn_server() -> (u16, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        std::thread::spawn(move || {
            use std::io::Write as _;
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let h = h.clone();
                // One thread per connection: Chrome may open idle preconnect sockets
                // that never send a request; only real HTTP requests are counted.
                std::thread::spawn(move || {
                    let mut buf = [0u8; 2048];
                    match s.read(&mut buf) {
                        Ok(n) if n > 0 => {
                            h.fetch_add(1, Ordering::SeqCst);
                            let png = png_bytes();
                            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", png.len());
                            let _ = s.write_all(&png);
                        }
                        _ => {}
                    }
                });
            }
        });
        (port, hits)
    }

    fn png_bytes() -> Vec<u8> {
        // tiny base64 decoder, avoids a dev dependency
        let mut out = Vec::new();
        let (mut acc, mut bits) = (0u32, 0);
        for c in PNG_B64.bytes() {
            let v = match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => continue,
            } as u32;
            acc = (acc << 6) | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
                acc &= (1 << bits) - 1;
            }
        }
        out
    }

    const SCRIPT_HTML: &str = r#"<html><body onload="document.getElementById('t').textContent='ONLOADCHANGED'">
        <p id="t">ORIGINALTEXT</p><p id="u">UNTOUCHED</p>
        <script>document.getElementById('u').textContent='SCRIPTCHANGED';</script>
        <img src="x" onerror="document.getElementById('u').textContent='ERRORCHANGED'">
        </body></html>"#;

    #[test]
    fn secure_does_not_run_scripts() {
        if !env_ok() { return; }
        let text = text_of(&manager().print_to_pdf(SCRIPT_HTML, true).unwrap());
        assert!(text.contains("ORIGINALTEXT") && text.contains("UNTOUCHED"), "{text}");
        assert!(!text.contains("CHANGED"), "script ran in secure mode: {text}");
    }

    #[test]
    fn legacy_still_runs_scripts() {
        if !env_ok() { return; }
        let text = text_of(&manager().print_to_pdf(SCRIPT_HTML, false).unwrap());
        // Inline handlers run in legacy mode (Page.setDocumentContent does not run <script> tags).
        assert!(text.contains("ONLOADCHANGED") && text.contains("ERRORCHANGED"), "legacy mode must keep JS on: {text}");
    }

    #[test]
    fn secure_blocks_http_requests() {
        if !env_ok() { return; }
        let (port, hits) = spawn_server();
        let html = format!(
            r#"<html><body><p>HTTPTEST</p><img src="http://127.0.0.1:{port}/x.png">
            <link rel="stylesheet" href="http://127.0.0.1:{port}/s.css"></body></html>"#
        );
        let blocked_before = BLOCKED_REQUESTS.load(Ordering::SeqCst);
        let pdf = manager().print_to_pdf(&html, true).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(text_of(&pdf).contains("HTTPTEST"));
        // Chrome itself refuses loopback targets from this kind of document, so a
        // zero hit count alone proves little: the interceptor must have seen and failed both requests.
        assert!(BLOCKED_REQUESTS.load(Ordering::SeqCst) >= blocked_before + 2, "interceptor did not fail the img/link requests");
        assert_eq!(hits.load(Ordering::SeqCst), 0, "secure mode made a network request");

        // Meta refresh to the network must not reach it either (the page may end on an error page).
        let refresh = format!(
            r#"<html><head><meta http-equiv="refresh" content="0;url=http://127.0.0.1:{port}/r"></head><body>R</body></html>"#
        );
        let _ = manager().print_to_pdf(&refresh, true);
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(hits.load(Ordering::SeqCst), 0, "secure mode followed a meta refresh to the network");
    }

    #[test]
    fn test_server_counts_requests() {
        let (port, hits) = spawn_server();
        let mut c = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut resp = Vec::new();
        let _ = c.read_to_end(&mut resp);
        assert!(resp.starts_with(b"HTTP/1.1 200"));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    /// Legacy mode keeps the network. Chrome refuses loopback/private targets
    /// from these documents even without any hardening, so a local server cannot be used:
    /// this test needs internet and is skipped without it.
    #[test]
    fn legacy_keeps_network_access_and_secure_blocks_it() {
        if !env_ok() { return; }
        let online = std::net::TcpStream::connect_timeout(
            &"1.1.1.1:443".parse().unwrap(), Duration::from_secs(3)).is_ok();
        if !online { eprintln!("SKIPPED: no internet"); return; }
        let html = r#"<html><body><p>LEGACYNET</p>
            <img src="https://www.google.com/images/branding/googlelogo/1x/googlelogo_color_272x92dp.png"></body></html>"#;
        let has_image = |pdf: &[u8]| pdf.windows(6).any(|w| w == b"/Image");
        assert!(has_image(&manager().print_to_pdf(html, false).unwrap()), "legacy mode must load external images");
        // Secure: the interceptor fails the request (the PDF may still hold the browser's broken-image icon).
        let before = BLOCKED_REQUESTS.load(Ordering::SeqCst);
        manager().print_to_pdf(html, true).unwrap();
        assert!(BLOCKED_REQUESTS.load(Ordering::SeqCst) > before, "secure mode did not block the external image");
    }

    #[test]
    fn secure_blocks_file_iframe() {
        if !env_ok() { return; }
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("secret.html");
        std::fs::write(&secret, "<html><body>FILESECRETMARKER</body></html>").unwrap();
        let html = format!(
            r#"<html><body><p>IFRAMETEST</p><iframe src="file://{}"></iframe>
            <iframe src="file:///etc/passwd"></iframe><object data="file://{}"></object></body></html>"#,
            secret.display(), secret.display()
        );
        // Chrome itself already refuses file: frames from an about:blank document,
        // so the interceptor may not even see these requests; the result must be the same.
        let text = text_of(&manager().print_to_pdf(&html, true).unwrap());
        assert!(text.contains("IFRAMETEST"));
        assert!(!text.contains("FILESECRETMARKER"), "file: iframe loaded: {text}");
        assert!(!text.contains("root:"), "/etc/passwd loaded: {text}");
    }

    #[test]
    fn secure_renders_normal_html_with_inline_css_and_data_uri() {
        if !env_ok() { return; }
        let html = format!(
            r#"<html><head><style>body{{font-family:sans-serif}} h1{{color:#c00}}</style></head>
            <body><h1>NORMALRENDER</h1>
            <img src="data:image/png;base64,{PNG_B64}" width="50" height="50">
            <div style="width:40px;height:40px;background:url('data:image/png;base64,{PNG_B64}')"></div>
            </body></html>"#
        );
        for secure in [true, false] {
            let pdf = manager().print_to_pdf(&html, secure).unwrap();
            assert!(pdf.starts_with(b"%PDF"));
            assert!(text_of(&pdf).contains("NORMALRENDER"));
            // the PDF embeds the image (an /Image XObject), also in secure mode
            assert!(pdf.windows(6).any(|w| w == b"/Image"), "no image in PDF (secure={secure})");
        }
    }
}
