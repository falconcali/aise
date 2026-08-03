use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Router, response::Response};
use tower_http::services::ServeDir;

use crate::api::session::{create_session, delete_session, list_sessions};
use crate::api::state::AppState;
use crate::api::turn::run_turn;
use crate::config::ServerConfig;

pub fn router(state: Arc<AppState>, config: &ServerConfig) -> Router {
    let assets_dir = config.resolved_assets_dir();
    Router::new()
        .route("/", get(serve_index))
        .nest_service("/assets", ServeDir::new(assets_dir))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/{id}", delete(delete_session))
        .route("/api/sessions/{id}/turns", post(run_turn))
        .with_state(state)
}

async fn serve_index(State(state): State<Arc<AppState>>) -> Response {
    let path = state.config.resolved_assets_dir().join("index.html");
    match tokio::fs::read_to_string(path).await {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}
