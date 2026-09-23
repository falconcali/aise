use super::*;
use std::collections::HashSet;

#[test]
fn registry_matches_the_phase_one_contract() {
    let expected = [
        (
            ObservationStep::ExecuteStoryTurn,
            ObservationKind::Chain,
            "execute-story-turn (执行故事回合)",
        ),
        (
            ObservationStep::ResolveInteractionSession,
            ObservationKind::Retriever,
            "resolve-interaction-session (解析交互会话)",
        ),
        (
            ObservationStep::ValidateRequest,
            ObservationKind::Span,
            "validate-request (校验请求)",
        ),
        (
            ObservationStep::AdmitTurnTask,
            ObservationKind::Span,
            "admit-turn-task (准入回合任务)",
        ),
        (
            ObservationStep::CoordinateStoryTurn,
            ObservationKind::Span,
            "coordinate-story-turn (协调故事回合)",
        ),
        (ObservationStep::LoadStory, ObservationKind::Retriever, "load-story (加载故事)"),
        (
            ObservationStep::CheckIdempotency,
            ObservationKind::Retriever,
            "check-idempotency (检查幂等性)",
        ),
        (
            ObservationStep::RunTurnPipelines,
            ObservationKind::Chain,
            "run-turn-pipelines (执行回合流水线)",
        ),
        (
            ObservationStep::InitializeTurn,
            ObservationKind::Chain,
            "initialize-turn (初始化回合)",
        ),
        (
            ObservationStep::PrepareContext,
            ObservationKind::Chain,
            "prepare-context (准备上下文)",
        ),
        (
            ObservationStep::LoadStorySnapshot,
            ObservationKind::Retriever,
            "load-story-snapshot (加载故事快照)",
        ),
        (
            ObservationStep::ActivateWorldInfo,
            ObservationKind::Retriever,
            "activate-world-info (激活世界信息)",
        ),
        (ObservationStep::PlanTurn, ObservationKind::Chain, "plan-turn (规划回合)"),
        (
            ObservationStep::ProjectNarrative,
            ObservationKind::Span,
            "project-narrative (投影叙事图)",
        ),
        (
            ObservationStep::GenerateWriterPlan,
            ObservationKind::Generation,
            "generate-writer-plan (生成写作计划)",
        ),
        (
            ObservationStep::RetrieveContext,
            ObservationKind::Retriever,
            "retrieve-context (检索上下文)",
        ),
        (
            ObservationStep::ThinkCharacters,
            ObservationKind::Chain,
            "think-characters (角色思考)",
        ),
        (
            ObservationStep::ThinkCharacter,
            ObservationKind::Generation,
            "think-character (角色思考)",
        ),
        (
            ObservationStep::GenerateStory,
            ObservationKind::Chain,
            "generate-story (生成故事)",
        ),
        (
            ObservationStep::DraftStoryText,
            ObservationKind::Generation,
            "draft-story-text (起草故事正文)",
        ),
        (
            ObservationStep::ExtractStoryState,
            ObservationKind::Chain,
            "extract-story-state (提取故事状态)",
        ),
        (
            ObservationStep::InferStoryState,
            ObservationKind::Generation,
            "infer-story-state (推断故事状态)",
        ),
        (
            ObservationStep::ValidateStory,
            ObservationKind::Evaluator,
            "validate-story (校验故事)",
        ),
        (ObservationStep::RepairStory, ObservationKind::Chain, "repair-story (修复故事)"),
        (
            ObservationStep::ReviseStoryText,
            ObservationKind::Generation,
            "revise-story-text (修订故事正文)",
        ),
        (ObservationStep::CommitTurn, ObservationKind::Chain, "commit-turn (提交回合)"),
        (ObservationStep::PersistTurn, ObservationKind::Tool, "persist-turn (持久化回合)"),
    ];

    assert_eq!(ObservationStep::ALL.len(), expected.len());
    for (actual_step, (expected_step, expected_kind, expected_name)) in
        ObservationStep::ALL.iter().copied().zip(expected)
    {
        assert_eq!(actual_step, expected_step);
        assert_eq!(actual_step.kind(), expected_kind);
        assert_eq!(actual_step.name(), expected_name);
    }
}

#[test]
fn registry_contains_each_step_once() {
    let unique = ObservationStep::ALL.iter().copied().collect::<HashSet<_>>();

    assert_eq!(unique.len(), ObservationStep::ALL.len());
    assert_eq!(ObservationStep::SCHEMA_VERSION, "1");
}

#[test]
fn every_name_uses_the_stable_bilingual_shape() {
    for step in ObservationStep::ALL {
        let name = step.name();
        let (stable, translated) = name.split_once(" (").unwrap();

        assert!(!stable.is_empty());
        assert!(
            stable
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        );
        assert!(translated.ends_with(')'));
        assert!(translated.len() > 1);
    }
}
