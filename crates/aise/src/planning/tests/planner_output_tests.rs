use super::*;
use crate::config::PlannerConfig;

#[test]
fn schema_omits_schema_uri_and_disallows_extra_fields() {
    let config = PlannerConfig::default();
    let schema = writer_planner_output_schema(&config);
    assert!(schema.get("$schema").is_none());
    assert_eq!(schema["additionalProperties"], false);
}

#[test]
fn dto_rejects_unknown_fields() {
    let raw = serde_json::json!({
        "story_goal": "goal",
        "writer_context_gaps": [],
        "character_context_gaps": [],
        "character_think_requests": [],
        "extra": true
    });
    let parsed: Result<WriterPlannerOutputDto, _> = serde_json::from_value(raw);
    assert!(parsed.is_err());
}

fn valid_dto() -> WriterPlannerOutputDto {
    WriterPlannerOutputDto {
        interpreted_player_contribution: InterpretedPlayerContributionDto {
            units: vec![PlayerContributionUnitDto {
                kind: PlayerContributionKindDto::PrivateState,
                content: "player feels afraid".to_owned(),
            }],
        },
        story_goal: "advance the plot".to_owned(),
        writer_context_gaps: vec![PlannerWriterContextGapDto {
            target_id: "fact_0001".to_owned(),
            reason: "needed for continuity".to_owned(),
        }],
        character_context_gaps: vec![],
        character_think_requests: vec![CharacterThinkRequestDto {
            role_id: "npc_guard".to_owned(),
            reason: "reacts to player".to_owned(),
        }],
    }
}

#[test]
fn contract_validate_accepts_well_formed_output() {
    let config = PlannerConfig::default();
    let contract = writer_planner_contract(&config);
    assert!((contract.validate)(&valid_dto()).is_ok());
}

#[test]
fn contract_validate_rejects_empty_story_goal() {
    let config = PlannerConfig::default();
    let contract = writer_planner_contract(&config);
    let mut dto = valid_dto();
    dto.story_goal = "   ".to_owned();
    assert!((contract.validate)(&dto).is_err());
}

#[test]
fn contract_validate_rejects_oversized_story_goal() {
    let config = PlannerConfig {
        max_goal_bytes: 4,
        ..PlannerConfig::default()
    };
    let contract = writer_planner_contract(&config);
    let mut dto = valid_dto();
    dto.story_goal = "way too long".to_owned();
    assert!((contract.validate)(&dto).is_err());
}

#[test]
fn contract_validate_rejects_too_many_context_gaps() {
    let config = PlannerConfig {
        max_context_gaps: 1,
        ..PlannerConfig::default()
    };
    let contract = writer_planner_contract(&config);
    let mut dto = valid_dto();
    dto.writer_context_gaps.push(PlannerWriterContextGapDto {
        target_id: "fact_0002".to_owned(),
        reason: "second gap".to_owned(),
    });
    assert!((contract.validate)(&dto).is_err());
}

#[test]
fn contract_validate_rejects_empty_gap_fields() {
    let config = PlannerConfig::default();
    let contract = writer_planner_contract(&config);
    let mut dto = valid_dto();
    dto.writer_context_gaps[0].reason = String::new();
    assert!((contract.validate)(&dto).is_err());
}

#[test]
fn contract_validate_rejects_empty_character_think_request_fields() {
    let config = PlannerConfig::default();
    let contract = writer_planner_contract(&config);
    let mut dto = valid_dto();
    dto.character_think_requests[0].role_id = String::new();
    assert!((contract.validate)(&dto).is_err());
}
