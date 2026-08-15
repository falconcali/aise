use crate::api::state::AppState;
use crate::error::ApiError;
use aise::story::character_card_service::{CharacterCardImportError, CharacterCardService};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct ValidationResponse {
    pub valid: bool,
    pub issues: Vec<ValidationIssueView>,
}

#[derive(Debug, Serialize)]
pub struct ValidationIssueView {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct CharacterCardInfoView {
    pub character_id: String,
    pub name: String,
    pub creator: Option<String>,
    pub version: String,
    pub digest: String,
}

impl From<aise::persistence::CharacterCardInfo> for CharacterCardInfoView {
    fn from(info: aise::persistence::CharacterCardInfo) -> Self {
        Self {
            character_id: info.character_id.to_string(),
            name: info.name.to_string(),
            creator: info.creator.map(|creator| creator.to_string()),
            version: info.version.to_string(),
            digest: info.digest.to_string(),
        }
    }
}

fn character_card_service(state: &AppState) -> Result<&Arc<CharacterCardService>, ApiError> {
    state
        .character_card_service
        .as_ref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("character card service not initialized")))
}

pub async fn validate_character_card(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Json<ValidationResponse>, ApiError> {
    let service = character_card_service(&state)?;
    let report = service.validate(&body);
    Ok(Json(ValidationResponse {
        valid: report.valid,
        issues: report
            .issues
            .into_iter()
            .map(|issue| ValidationIssueView {
                code: issue.code.to_string(),
                path: issue.path,
                message: issue.message,
            })
            .collect(),
    }))
}

pub async fn import_character_card(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<(StatusCode, Json<CharacterCardInfoView>), ApiError> {
    let service = character_card_service(&state)?;
    let info = service.import(&body).await.map_err(map_import_error)?;
    Ok((StatusCode::CREATED, Json(CharacterCardInfoView::from(info))))
}

pub async fn list_character_cards(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<CharacterCardInfoView>>, ApiError> {
    let service = character_card_service(&state)?;
    let infos = service.list().await.map_err(map_import_error)?;
    Ok(Json(infos.into_iter().map(CharacterCardInfoView::from).collect()))
}

fn map_import_error(error: CharacterCardImportError) -> ApiError {
    match error {
        CharacterCardImportError::Invalid(report) => {
            ApiError::Unprocessable(format!("character card validation failed: {:?}", report.issues))
        }
        CharacterCardImportError::Store(store_error) => ApiError::Internal(anyhow::anyhow!(store_error)),
    }
}
