use crate::api::bind::bind_story;
use crate::api::pack::{delete_pack, export_pack, import_pack, list_packs, validate_pack};
use crate::api::session::{create_session, delete_session, list_sessions};
use crate::api::state::AppState;
use crate::api::story::{create_story, create_story_instance, get_story};
use crate::api::turn::{get_turn_result, run_turn};
use crate::config::ServerConfig;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::{Router, response::Response};
use std::sync::Arc;
use tower_http::services::ServeDir;

pub fn router(state: Arc<AppState>, config: &ServerConfig) -> Router {
    let assets_dir = config.resolved_assets_dir();
    Router::new()
        .route("/", get(serve_index))
        .nest_service("/assets", ServeDir::new(assets_dir))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/{id}", delete(delete_session))
        .route("/api/sessions/{id}/story", put(bind_story))
        .route("/api/sessions/{id}/turns", post(run_turn))
        .route("/api/stories", post(create_story))
        .route("/api/stories/{id}", get(get_story))
        .route("/api/stories/{id}/turn-results/{idempotency_key}", get(get_turn_result))
        .route("/api/packs/validate", post(validate_pack))
        .route("/api/packs", get(list_packs).post(import_pack))
        .route("/api/packs/{id}", get(export_pack).delete(delete_pack))
        .route("/api/story-instances", post(create_story_instance))
        .with_state(state)
}

async fn serve_index(State(state): State<Arc<AppState>>) -> Response {
    let path = state.config.resolved_assets_dir().join("index.html");
    match tokio::fs::read_to_string(path).await {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}
