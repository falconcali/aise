use crate::character::character_model::CharacterThought;
use crate::context::ctx_model::{BaselineContext, ContextItem};
use crate::domain::ids::{StoryId, TurnId};
use crate::planning::writer_planner::WriterPlan;
use crate::runtime::trace::ExecutionTrace;
use crate::runtime::turn_budget::TurnBudget;
use crate::story::story_model::StoryDraft;
use crate::validation::ValidationResult;

/// Shared context for one Turn (Architecture.md §5).
///
/// Lives only for the duration of the Turn: created by `TurnRuntime`, mutated
/// by each pipeline, destroyed after commit. MUST NOT be persisted directly
/// or shared across Turns (R-AISE-03).
pub struct TurnExecutionContext {
    pub story_id: StoryId,
    pub turn_id: TurnId,
    pub player_input: String,

    // AI baseline cognition, set by BaselineContextBuilder (§7).
    pub baseline_ctx: BaselineContext,
    // Planner output (§8).
    pub plan: Option<WriterPlan>,
    // Retrieval results (§9).
    pub retrieved_ctx: Vec<ContextItem>,
    // Character viewpoint simulations (§10).
    pub character_thoughts: Vec<CharacterThought>,
    // Current story result (§11).
    pub draft: Option<StoryDraft>,
    // Validation outcome (§13).
    pub validation: ValidationResult,

    pub budget: TurnBudget,
    pub trace: ExecutionTrace,
}

impl TurnExecutionContext {
    pub fn new(story_id: StoryId, player_input: String) -> Self {
        Self {
            turn_id: TurnId::from(""), // assigned by TurnInitializer
            story_id,
            player_input,
            baseline_ctx: BaselineContext::default(),
            plan: None,
            retrieved_ctx: Vec::new(),
            character_thoughts: Vec::new(),
            draft: None,
            validation: ValidationResult::default(),
            budget: TurnBudget::default(),
            trace: ExecutionTrace::default(),
        }
    }
}
