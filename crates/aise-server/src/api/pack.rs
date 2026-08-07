use crate::api::state::AppState;
use crate::error::ApiError;
use aise::story::pack_service::{
    AssetExportError, AssetImportError, AssetInput, PackExport, PackExportFormat, PackService, PackSummary,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct PackQuery {
    pub content_type: PackContentType,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackContentType {
    Json,
    AisePack,
}

#[derive(Debug, Serialize)]
pub struct ImportPackResponse {
    pub pack_id: String,
    pub pack_key: String,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Serialize)]
pub struct PackSummaryView {
    pub pack_id: String,
    pub pack_key: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub tags: Vec<String>,
    pub digest: String,
}

impl From<PackSummary> for PackSummaryView {
    fn from(summary: PackSummary) -> Self {
        Self {
            pack_id: summary.pack_id.to_string(),
            pack_key: summary.pack_key.to_string(),
            title: summary.title,
            author: summary.author,
            version: summary.version.to_string(),
            description: summary.description,
            tags: summary.tags,
            digest: summary.digest.to_string(),
        }
    }
}

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

fn input_from(content_type: PackContentType, body: &[u8]) -> AssetInput<'_> {
    match content_type {
        PackContentType::Json => AssetInput::Json(body),
        PackContentType::AisePack => AssetInput::Pack(body),
    }
}

pub async fn validate_pack(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PackQuery>,
    body: Bytes,
) -> Result<Json<ValidationResponse>, ApiError> {
    let pack_service = state
        .pack_service
        .as_ref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("pack service not initialized")))?;
    let input = input_from(query.content_type, &body);
    let report = pack_service.validate(input);
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

pub async fn import_pack(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PackQuery>,
    body: Bytes,
) -> Result<(StatusCode, Json<ImportPackResponse>), ApiError> {
    let pack_service = state
        .pack_service
        .as_ref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("pack service not initialized")))?;
    let input = input_from(query.content_type, &body);
    let info = pack_service.import(input).await.map_err(|error| match error {
        AssetImportError::Invalid(report) => {
            ApiError::Unprocessable(format!("pack validation failed: {:?}", report.issues))
        }
        AssetImportError::Store(store_error) => ApiError::Internal(anyhow::anyhow!(store_error)),
        AssetImportError::Io { code } => ApiError::Internal(anyhow::anyhow!("pack import I/O failure: {code}")),
    })?;
    Ok((
        StatusCode::CREATED,
        Json(ImportPackResponse {
            pack_id: info.pack_id.to_string(),
            pack_key: info.pack_key.to_string(),
            version: info.version.to_string(),
            digest: info.digest.to_string(),
        }),
    ))
}

pub async fn list_packs(State(state): State<Arc<AppState>>) -> Result<Json<Vec<PackSummaryView>>, ApiError> {
    let pack_service = state
        .pack_service
        .as_ref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("pack service not initialized")))?;
    let summaries = pack_service.list().await.map_err(|error| match error {
        AssetExportError::Store(store_error) => ApiError::Internal(anyhow::anyhow!(store_error)),
        AssetExportError::Io { code } => ApiError::Internal(anyhow::anyhow!("pack list I/O failure: {code}")),
        AssetExportError::NotFound => ApiError::Internal(anyhow::anyhow!("pack list unexpectedly empty")),
    })?;
    Ok(Json(summaries.into_iter().map(PackSummaryView::from).collect()))
}

pub async fn delete_pack(
    State(state): State<Arc<AppState>>,
    Path(pack_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let pack_id =
        aise::domain::asset::ids::PackId::try_new(pack_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let pack_service = state
        .pack_service
        .as_ref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("pack service not initialized")))?;
    let deleted = pack_service.delete(&pack_id).await.map_err(|error| match error {
        AssetExportError::NotFound => ApiError::NotFound("story pack".into()),
        AssetExportError::Store(aise::persistence::StoreError::ConstraintViolation { constraint }) => {
            ApiError::Conflict(format!("story pack is in use by one or more story instances ({constraint})"))
        }
        AssetExportError::Store(store_error) => ApiError::Internal(anyhow::anyhow!(store_error)),
        AssetExportError::Io { code } => ApiError::Internal(anyhow::anyhow!("pack delete I/O failure: {code}")),
    })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound("story pack".into()))
    }
}

pub async fn export_pack(
    State(state): State<Arc<AppState>>,
    Path(pack_id): Path<String>,
) -> Result<Response, ApiError> {
    let pack_id =
        aise::domain::asset::ids::PackId::try_new(pack_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let pack_service = state
        .pack_service
        .as_ref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("pack service not initialized")))?;
    let format = PackExportFormat::Json;
    let export = pack_service.export(&pack_id, format).await.map_err(|error| match error {
        AssetExportError::NotFound => ApiError::NotFound("story pack".into()),
        AssetExportError::Store(store_error) => ApiError::Internal(anyhow::anyhow!(store_error)),
        AssetExportError::Io { code } => ApiError::Internal(anyhow::anyhow!("pack export I/O failure: {code}")),
    })?;
    match export {
        PackExport::Json(bytes) => {
            let content_type = "application/json"
                .parse::<axum::http::HeaderValue>()
                .map_err(|_| ApiError::Internal(anyhow::anyhow!("invalid content type header")))?;
            let mut response = Response::new(bytes.into());
            response.headers_mut().insert(axum::http::header::CONTENT_TYPE, content_type);
            Ok(response)
        }
        PackExport::AisePack(_) => Err(ApiError::Internal(anyhow::anyhow!(
            "pack container export is not yet supported"
        ))),
    }
}

#[allow(dead_code)]
pub(crate) fn _pack_api_anchor(_: &PackService, _: &AssetInput<'_>) {}
