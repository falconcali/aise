use crate::domain::asset::ids::Sha256Digest;
use crate::domain::turn::{
    NarrativeConditionResult, StoryCandidateVersion, StoryStateExtractionDto, StoryStateExtractionEnvelope,
    StoryStateExtractionLimits,
};
use crate::llm::error::{LlmError, LlmProtocolErrorKind};
use crate::llm::gateway::LlmGateway;
use crate::llm::output_contract::{LlmOutputContract, LlmOutputViolation};
use crate::prompt::{PromptCompositionInput, PromptProfile};
use crate::story::story_state_extractor_prompt::{
    DefaultStoryStateExtractorPromptContextProjector, StoryStateExtractorProjectionError,
    StoryStateExtractorPromptContextProjector,
};
use crate::turn::turn_context::TurnExecutionContext;
use crate::turn::turn_contract::{LlmCallPurpose, TurnPhase};
use crate::turn::turn_error::{TurnExecutionError, TurnFailureKind};
use crate::turn::turn_pipeline::{TurnExecutionPipeline, TurnStage};
use crate::turn::turn_validation::{
    BoundedValidationIssues, ValidationIssue, ValidationIssueClass, ValidationIssueCode, ValidationRemedy,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::Instrument;

const STORY_STATE_EXTRACTION_CONTRACT_NAME: &str = "story_state_extraction.v2";

pub struct StoryStateExtractor {
    gateway: Arc<LlmGateway>,
    projector: Arc<dyn StoryStateExtractorPromptContextProjector>,
}

impl StoryStateExtractor {
    pub fn new(gateway: Arc<LlmGateway>) -> Self {
        Self {
            gateway,
            projector: Arc::new(DefaultStoryStateExtractorPromptContextProjector),
        }
    }

    pub fn with_projector(
        gateway: Arc<LlmGateway>,
        projector: Arc<dyn StoryStateExtractorPromptContextProjector>,
    ) -> Self {
        Self { gateway, projector }
    }
}

#[async_trait]
impl TurnExecutionPipeline for StoryStateExtractor {
    fn stage(&self) -> TurnStage {
        TurnStage::StoryStateExtractor
    }

    async fn execute(&self, ctx: &mut TurnExecutionContext) -> Result<(), TurnExecutionError> {
        let is_reextraction = ctx.phase() == TurnPhase::StateReextractionRequired;
        let projection = self.projector.project(ctx).map_err(map_projection_error)?;
        let request = PromptCompositionInput {
            profile: PromptProfile::StoryStateExtractor,
            rc_vars: projection.rc_vars,
            fti_vars: projection.fti_vars,
        };
        let max_output_tokens = ctx
            .budget()
            .state_extractor_max_output_tokens()
            .min(ctx.budget().remaining_output_tokens())
            .min(u64::from(u32::MAX)) as u32;
        tracing::info!(
            prompt_profile = "story_state_extractor",
            is_reextraction,
            "story state extractor prompt projected"
        );
        let limits = ctx.budget().state_extraction_limits();
        let scope = ctx.llm_call_scope(TurnStage::StoryStateExtractor);
        let span = tracing::info_span!(
            "story_state_extractor.extract",
            prompt_profile = "story_state_extractor",
            is_reextraction
        );
        let contract = story_state_extraction_contract(limits);
        let outcome = self
            .gateway
            .complete_structured_composed(
                scope,
                request,
                max_output_tokens,
                LlmCallPurpose::StoryStateExtraction,
                contract,
            )
            .instrument(span)
            .await;
        match outcome {
            Ok(structured) => {
                let dto = structured.value;
                tracing::info!(
                    prompt_profile = "story_state_extractor",
                    is_reextraction,
                    output_bytes = structured.completion.text.len(),
                    new_role_count = dto.new_roles.len(),
                    role_state_count = dto.role_states.len(),
                    relationship_state_count = dto.relationship_states.len(),
                    "story state extractor output decoded"
                );
                let expected_graph_revision = ctx
                    .narrative_projection()
                    .map(|projection| projection.expected_graph_revision)
                    .unwrap_or_default();
                let narrative_condition_results = dto
                    .narrative_condition_judgments
                    .iter()
                    .filter_map(|judgment| {
                        let condition_key =
                            crate::domain::asset::ids::NarrativeConditionKey::try_new(judgment.condition_key.clone())
                                .ok()?;
                        let evidence = crate::domain::asset::validation::BoundedText::try_new(
                            judgment.evidence.clone(),
                            "narrative_condition_evidence",
                            limits.max_condition_evidence_bytes,
                        )
                        .ok()?;
                        let reason = crate::domain::asset::validation::BoundedText::try_new(
                            judgment.reason.clone(),
                            "narrative_condition_reason",
                            limits.max_condition_reason_bytes,
                        )
                        .ok()?;
                        Some(NarrativeConditionResult {
                            condition_key,
                            status: judgment.status,
                            evidence,
                            reason,
                        })
                    })
                    .collect();
                let candidate_version = StoryCandidateVersion {
                    content_digest: Sha256Digest::from_bytes(
                        Sha256::digest(structured.completion.text.as_bytes()).into(),
                    ),
                    repair_attempt: u32::from(is_reextraction),
                };
                let envelope = StoryStateExtractionEnvelope {
                    candidate_version,
                    expected_graph_revision,
                    state: dto,
                    narrative_condition_results,
                };
                if is_reextraction {
                    ctx.replace_state_extraction(envelope)
                } else {
                    ctx.set_state_extraction(envelope)
                }
            }
            Err(LlmError::Protocol {
                kind: LlmProtocolErrorKind::InvalidStructuredOutput,
            }) => {
                tracing::warn!(
                    prompt_profile = "story_state_extractor",
                    is_reextraction,
                    "story state extractor output decode or contract validation failed"
                );
                let issue = ValidationIssue {
                    code: ValidationIssueCode::ExtractionSchemaInvalid,
                    class: ValidationIssueClass::Extraction,
                    remedy: ValidationRemedy::ReextractState,
                    message: "state extraction output is invalid".to_owned(),
                    location: None,
                };
                let issues = BoundedValidationIssues::try_new(vec![issue], ctx.budget().max_validation_issues())?;
                ctx.record_state_extraction_failure(issues)
            }
            Err(error) => Err(TurnExecutionError::new(
                TurnFailureKind::Llm,
                "llm_error",
                Some(TurnStage::StoryStateExtractor),
                error.to_string(),
            )),
        }
    }
}

fn story_state_extraction_contract(limits: StoryStateExtractionLimits) -> LlmOutputContract<StoryStateExtractionDto> {
    let schema = StoryStateExtractionDto::json_schema(limits);
    let compact_prompt_shape = StoryStateExtractionDto::compact_prompt_shape();
    LlmOutputContract {
        name: STORY_STATE_EXTRACTION_CONTRACT_NAME,
        schema: Arc::new(schema),
        compact_prompt_shape: Arc::from(compact_prompt_shape.as_str()),
        validate: Arc::new(move |dto: &StoryStateExtractionDto| {
            let violation =
                |reason: &str| Err(LlmOutputViolation::new(STORY_STATE_EXTRACTION_CONTRACT_NAME, reason.to_owned()));
            if dto.new_roles.len() > limits.max_new_roles {
                return violation("new_roles exceeds the configured maximum");
            }
            if dto.role_states.len() > limits.max_role_states {
                return violation("role_states exceeds the configured maximum");
            }
            if dto.relationship_states.len() > limits.max_relationship_states {
                return violation("relationship_states exceeds the configured maximum");
            }
            if dto.narrative_condition_judgments.len() > limits.max_condition_queries {
                return violation("narrative_condition_judgments exceeds the configured maximum");
            }
            if dto.cast_policy_violations.len() > limits.max_cast_policy_violations {
                return violation("cast_policy_violations exceeds the configured maximum");
            }
            Ok(())
        }),
    }
}

fn map_projection_error(error: StoryStateExtractorProjectionError) -> TurnExecutionError {
    let code = match error {
        StoryStateExtractorProjectionError::MissingStory => "missing_story",
        StoryStateExtractorProjectionError::MissingSnapshot => "missing_snapshot",
        StoryStateExtractorProjectionError::ValidationDoesNotRequireReextraction => {
            "validation_does_not_require_reextraction"
        }
        StoryStateExtractorProjectionError::EmptyValidationIssues => "empty_validation_issues",
        StoryStateExtractorProjectionError::Invariant { .. } => "story_state_extractor_prompt_invariant",
        StoryStateExtractorProjectionError::RequiredPromptDataExceedsBudget => {
            "story_state_extractor_prompt_budget_exceeded"
        }
    };
    TurnExecutionError::new(
        TurnFailureKind::InvariantViolation,
        code,
        Some(TurnStage::StoryStateExtractor),
        error.to_string(),
    )
}
