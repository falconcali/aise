use crate::api::state::AppState;
use crate::error::ApiError;
use aise::context::activation::{ActivationPreviewSpec, ActivationPreviewTarget, KnowledgeActivationPreviewService};
use aise::domain::ids::StoryId;
use aise::domain::knowledge::KnowledgeSourceId;
use aise::domain::knowledge::activation::GenerationTrigger;
use aise::domain::turn::KnowledgeDelivery;
use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationPreviewRequest {
    pub player_contribution: String,
    #[serde(default = "default_generation_trigger")]
    pub generation_trigger: GenerationTrigger,
    #[serde(default)]
    pub external_targets: Vec<ActivationPreviewTargetRequest>,
}

fn default_generation_trigger() -> GenerationTrigger {
    GenerationTrigger::Normal
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationPreviewTargetRequest {
    pub source_id: KnowledgeSourceId,
    pub delivery: KnowledgeDelivery,
    #[serde(default)]
    pub mandatory: bool,
}

pub async fn preview_activation(
    State(state): State<Arc<AppState>>,
    Path(story_id): Path<String>,
    Json(request): Json<ActivationPreviewRequest>,
) -> Result<Json<aise::context::activation::ActivationPreviewResult>, ApiError> {
    let story_id = StoryId::try_new(story_id).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let service: &KnowledgeActivationPreviewService = state
        .activation_preview
        .as_deref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("activation preview service not initialized")))?;
    if matches!(request.generation_trigger, GenerationTrigger::Repair) {
        return Err(ApiError::BadRequest("activation_preview_invalid_request".into()));
    }
    let external_targets = request
        .external_targets
        .into_iter()
        .map(|target| ActivationPreviewTarget {
            source_id: target.source_id,
            delivery: target.delivery,
            mandatory: target.mandatory,
        })
        .collect();
    let result = service
        .preview(ActivationPreviewSpec {
            story_id,
            player_contribution: request.player_contribution,
            generation_trigger: request.generation_trigger,
            external_targets,
        })
        .await
        .map_err(map_preview_error)?;
    Ok(Json(result))
}

fn map_preview_error(error: aise::context::activation::ActivationPreviewError) -> ApiError {
    match error {
        aise::context::activation::ActivationPreviewError::Store(aise::persistence::StoreError::NotFound) => {
            ApiError::NotFound("story_not_found".into())
        }
        aise::context::activation::ActivationPreviewError::UnauthorizedTarget => {
            ApiError::Forbidden("activation_preview_target_unauthorized".into())
        }
        aise::context::activation::ActivationPreviewError::ResponseLimitExceeded => {
            ApiError::Unprocessable("activation_preview_response_limit".into())
        }
        aise::context::activation::ActivationPreviewError::Store(_) => {
            ApiError::ServiceUnavailable("store_unavailable".into())
        }
        aise::context::activation::ActivationPreviewError::InvalidLimits
        | aise::context::activation::ActivationPreviewError::MissingMetadata
        | aise::context::activation::ActivationPreviewError::Serialization => {
            ApiError::BadRequest("activation_preview_invalid_request".into())
        }
        aise::context::activation::ActivationPreviewError::Activation(error) => {
            ApiError::Unprocessable(error.to_string())
        }
    }
}
