use crate::character::character_model::CharacterThought;
use crate::context::ctx_model::{BaselineContext, ContextItem};
use crate::domain::ids::{StoryId, TurnId};
use crate::planning::writer_planner::WriterPlan;
use crate::runtime::trace::TraceRecorder;
use crate::runtime::turn_budget::TurnBudget;
use crate::story::story_model::StoryDraft;
use crate::validation::ValidationResult;

pub struct TurnExecutionContext {
    pub story_id: StoryId,
    pub turn_id: TurnId,
    pub player_input: String,

    pub baseline_ctx: BaselineContext,

    pub plan: Option<WriterPlan>,

    pub retrieved_ctx: Vec<ContextItem>,

    pub character_thoughts: Vec<CharacterThought>,

    pub draft: Option<StoryDraft>,

    pub validation: ValidationResult,

    pub budget: TurnBudget,
    pub trace: TraceRecorder,
}

impl TurnExecutionContext {
    pub fn new(story_id: StoryId, player_input: String) -> Self {
        Self {
            turn_id: TurnId::from(""),
            story_id,
            player_input,
            baseline_ctx: BaselineContext::default(),
            plan: None,
            retrieved_ctx: Vec::new(),
            character_thoughts: Vec::new(),
            draft: None,
            validation: ValidationResult::default(),
            budget: TurnBudget::default(),
            trace: TraceRecorder::new(),
        }
    }
}
