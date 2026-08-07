use aise::core::turn_contract::LlmCallPurpose;
use aise::domain::asset::validation::BoundedText;
use aise::domain::ids::CharacterId;
use aise::prompt::{
    CharacterThinkContext, ModelRequest, NarrativeValidatorContext, PromptProfile, RuntimeContextEncoder,
    StoryGeneratorContext, StoryRepairerContext, TrustedSystemPrompt, UntrustedContextMessage, WriterPlannerContext,
};

fn bounded(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", 4096).unwrap()
}

#[test]
fn writer_planner_request_has_typed_profile() {
    let request = ModelRequest::writer_planner(
        WriterPlannerContext {
            baseline: aise::core::turn_data::BaselineContext::default(),
        },
        512,
    );
    assert_eq!(request.profile(), PromptProfile::WriterPlanner);
    assert_eq!(request.purpose(), LlmCallPurpose::WriterPlan);
    assert_eq!(request.max_output_tokens(), 512);
}

#[test]
fn character_think_request_has_typed_profile() {
    let request = ModelRequest::character_think(
        CharacterThinkContext {
            baseline: aise::core::turn_data::BaselineContext::default(),
            character: aise::domain::character::CharacterState {
                id: CharacterId::from("char-1"),
                name: "Ada".into(),
                bio: "bio".into(),
                internal_state: aise::domain::character::InternalState {
                    goals: vec!["help".into()],
                    health: 100,
                    relationships: Vec::new(),
                },
            },
            player_input: bounded("input"),
            current_perception: Vec::new(),
        },
        256,
    );
    assert_eq!(request.profile(), PromptProfile::CharacterThink);
    assert_eq!(request.purpose(), LlmCallPurpose::CharacterThink);
}

#[test]
fn story_generator_request_has_typed_profile() {
    let request = ModelRequest::story_generator(
        StoryGeneratorContext {
            baseline: aise::core::turn_data::BaselineContext::default(),
            thoughts: Vec::new(),
            current_scene: None,
        },
        1024,
    );
    assert_eq!(request.profile(), PromptProfile::StoryGenerator);
    assert_eq!(request.purpose(), LlmCallPurpose::StoryGeneration);
}

#[test]
fn story_repairer_request_has_typed_profile() {
    let proposal = aise::core::story_proposal::StoryProposal::default();
    let request = ModelRequest::story_repairer(
        StoryRepairerContext {
            generator: StoryGeneratorContext {
                baseline: aise::core::turn_data::BaselineContext::default(),
                thoughts: Vec::new(),
                current_scene: None,
            },
            issues: Vec::new(),
            previous_proposal: proposal,
        },
        512,
    );
    assert_eq!(request.profile(), PromptProfile::StoryRepairer);
    assert_eq!(request.purpose(), LlmCallPurpose::StoryRepair);
}

#[test]
fn narrative_validator_request_has_typed_profile() {
    let request = ModelRequest::narrative_validator(NarrativeValidatorContext {
        baseline: aise::core::turn_data::BaselineContext::default(),
        proposal: aise::core::story_proposal::StoryProposal::default(),
    });
    assert_eq!(request.profile(), PromptProfile::NarrativeValidator);
    assert_eq!(request.purpose(), LlmCallPurpose::NarrativeValidation);
}

#[test]
fn trusted_prompt_is_distinct_from_untrusted_context() {
    let trusted = TrustedSystemPrompt::try_new("you are a narrator").unwrap();
    let untrusted = UntrustedContextMessage::new("{\"scene\":\"village\"}");
    assert!(trusted.as_str().contains("narrator"));
    assert!(untrusted.as_str().contains("village"));
}

#[test]
fn runtime_context_encoder_emits_json_message() {
    let encoder = RuntimeContextEncoder;
    let context = WriterPlannerContext {
        baseline: aise::core::turn_data::BaselineContext::default(),
    };
    let encoded = encoder.encode(&context).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(encoded.as_str()).unwrap();
    assert!(parsed.is_object());
}

#[test]
fn request_context_is_retrievable() {
    let request = ModelRequest::writer_planner(
        WriterPlannerContext {
            baseline: aise::core::turn_data::BaselineContext::default(),
        },
        512,
    );
    let context = request.context();
    assert!(context.baseline.recent_story.is_empty());
}
