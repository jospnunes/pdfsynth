use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod api;
mod core;
mod infra;

#[tokio::main]
async fn main() {
    // Configurar tracing com filtro por nível via RUST_LOG
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            // Default: info para pdfsynth, warn para outras crates
            "pdfsynth=info,tower_http=debug,warn".into()
        }))
        .with(tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true))
        .init();

    tracing::info!(
        event = "application_starting",
        version = env!("CARGO_PKG_VERSION"),
        "PDFSynth starting up"
    );

    let browser = match infra::browser::BrowserManager::new() {
        Ok(b) => {
            tracing::info!(event = "browser_initialized", "Browser manager initialized successfully");
            b
        }
        Err(e) => {
            tracing::error!(event = "browser_init_failed", error = %e, "Failed to initialize browser");
            panic!("Failed to initialize browser: {}", e);
        }
    };

    let template_engine = match infra::templates::TemplateEngine::new() {
        Ok(t) => {
            tracing::info!(event = "template_engine_initialized", "Template engine initialized successfully");
            t
        }
        Err(e) => {
            tracing::error!(event = "template_engine_init_failed", error = %e, "Failed to initialize template engine");
            panic!("Failed to initialize template engine: {}", e);
        }
    };

    let state = api::state::AppState { browser, template_engine };

    let app = Router::new()
        .route("/health", get(api::health::health_check))
        .route("/render/debug", axum::routing::post(api::render::render_html))
        .route("/render", axum::routing::post(api::render::render_pdf))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    
    tracing::info!(
        event = "server_listening",
        address = %addr,
        port = %port,
        "PDFSynth server started and listening for requests"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
        tracing::info!(event = "shutdown_signal_received", signal = "SIGINT", "Received Ctrl+C, initiating graceful shutdown");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
        tracing::info!(event = "shutdown_signal_received", signal = "SIGTERM", "Received SIGTERM, initiating graceful shutdown");
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
