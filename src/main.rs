use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

mod api;
mod core;
mod infra;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let browser = infra::browser::BrowserManager::new().expect("Failed to initialize browser");
    let template_engine = infra::templates::TemplateEngine::new().expect("Failed to initialize template engine");
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
    tracing::info!("listening on {}", addr);

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
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
