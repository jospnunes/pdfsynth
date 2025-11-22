use axum::extract::FromRef;
use crate::infra::{browser::BrowserManager, templates::TemplateEngine};

#[derive(Clone)]
pub struct AppState {
    pub browser: BrowserManager,
    pub template_engine: TemplateEngine,
}

impl FromRef<AppState> for BrowserManager {
    fn from_ref(state: &AppState) -> Self {
        state.browser.clone()
    }
}

impl FromRef<AppState> for TemplateEngine {
    fn from_ref(state: &AppState) -> Self {
        state.template_engine.clone()
    }
}
