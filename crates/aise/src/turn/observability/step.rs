#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObservationKind {
    Chain,
    Span,
    Generation,
    Retriever,
    Tool,
    Evaluator,
}

impl ObservationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chain => "chain",
            Self::Span => "span",
            Self::Generation => "generation",
            Self::Retriever => "retriever",
            Self::Tool => "tool",
            Self::Evaluator => "evaluator",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObservationStep {
    ExecuteStoryTurn,
    ResolveInteractionSession,
    ValidateRequest,
    AdmitTurnTask,
    CoordinateStoryTurn,
    LoadStory,
    CheckIdempotency,
    RunTurnPipelines,
    InitializeTurn,
    PrepareContext,
    LoadStorySnapshot,
    ActivateWorldInfo,
    PlanTurn,
    ProjectNarrative,
    GenerateWriterPlan,
    RetrieveContext,
    ThinkCharacters,
    ThinkCharacter,
    GenerateStory,
    DraftStoryText,
    ExtractStoryState,
    InferStoryState,
    ValidateStory,
    RepairStory,
    ReviseStoryText,
    CommitTurn,
    PersistTurn,
}

impl ObservationStep {
    pub const ALL: &'static [Self] = &[
        Self::ExecuteStoryTurn,
        Self::ResolveInteractionSession,
        Self::ValidateRequest,
        Self::AdmitTurnTask,
        Self::CoordinateStoryTurn,
        Self::LoadStory,
        Self::CheckIdempotency,
        Self::RunTurnPipelines,
        Self::InitializeTurn,
        Self::PrepareContext,
        Self::LoadStorySnapshot,
        Self::ActivateWorldInfo,
        Self::PlanTurn,
        Self::ProjectNarrative,
        Self::GenerateWriterPlan,
        Self::RetrieveContext,
        Self::ThinkCharacters,
        Self::ThinkCharacter,
        Self::GenerateStory,
        Self::DraftStoryText,
        Self::ExtractStoryState,
        Self::InferStoryState,
        Self::ValidateStory,
        Self::RepairStory,
        Self::ReviseStoryText,
        Self::CommitTurn,
        Self::PersistTurn,
    ];
    pub const SCHEMA_VERSION: &'static str = "1";

    pub const fn name(self) -> &'static str {
        match self {
            Self::ExecuteStoryTurn => "execute-story-turn (执行故事回合)",
            Self::ResolveInteractionSession => "resolve-interaction-session (解析交互会话)",
            Self::ValidateRequest => "validate-request (校验请求)",
            Self::AdmitTurnTask => "admit-turn-task (准入回合任务)",
            Self::CoordinateStoryTurn => "coordinate-story-turn (协调故事回合)",
            Self::LoadStory => "load-story (加载故事)",
            Self::CheckIdempotency => "check-idempotency (检查幂等性)",
            Self::RunTurnPipelines => "run-turn-pipelines (执行回合流水线)",
            Self::InitializeTurn => "initialize-turn (初始化回合)",
            Self::PrepareContext => "prepare-context (准备上下文)",
            Self::LoadStorySnapshot => "load-story-snapshot (加载故事快照)",
            Self::ActivateWorldInfo => "activate-world-info (激活世界信息)",
            Self::PlanTurn => "plan-turn (规划回合)",
            Self::ProjectNarrative => "project-narrative (投影叙事图)",
            Self::GenerateWriterPlan => "generate-writer-plan (生成写作计划)",
            Self::RetrieveContext => "retrieve-context (检索上下文)",
            Self::ThinkCharacters => "think-characters (角色思考)",
            Self::ThinkCharacter => "think-character (角色思考)",
            Self::GenerateStory => "generate-story (生成故事)",
            Self::DraftStoryText => "draft-story-text (起草故事正文)",
            Self::ExtractStoryState => "extract-story-state (提取故事状态)",
            Self::InferStoryState => "infer-story-state (推断故事状态)",
            Self::ValidateStory => "validate-story (校验故事)",
            Self::RepairStory => "repair-story (修复故事)",
            Self::ReviseStoryText => "revise-story-text (修订故事正文)",
            Self::CommitTurn => "commit-turn (提交回合)",
            Self::PersistTurn => "persist-turn (持久化回合)",
        }
    }

    pub const fn kind(self) -> ObservationKind {
        match self {
            Self::ExecuteStoryTurn
            | Self::RunTurnPipelines
            | Self::InitializeTurn
            | Self::PrepareContext
            | Self::PlanTurn
            | Self::ThinkCharacters
            | Self::GenerateStory
            | Self::ExtractStoryState
            | Self::RepairStory
            | Self::CommitTurn => ObservationKind::Chain,
            Self::ValidateRequest | Self::AdmitTurnTask | Self::CoordinateStoryTurn | Self::ProjectNarrative => {
                ObservationKind::Span
            }
            Self::GenerateWriterPlan
            | Self::ThinkCharacter
            | Self::DraftStoryText
            | Self::InferStoryState
            | Self::ReviseStoryText => ObservationKind::Generation,
            Self::ResolveInteractionSession
            | Self::LoadStory
            | Self::CheckIdempotency
            | Self::LoadStorySnapshot
            | Self::ActivateWorldInfo
            | Self::RetrieveContext => ObservationKind::Retriever,
            Self::PersistTurn => ObservationKind::Tool,
            Self::ValidateStory => ObservationKind::Evaluator,
        }
    }
}

#[cfg(test)]
#[path = "tests/step_tests.rs"]
mod tests;
