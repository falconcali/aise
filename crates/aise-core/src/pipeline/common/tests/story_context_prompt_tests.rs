use super::*;
use crate::core::tests::{store_with_default_pack, turn_evaluation};
use crate::core::{
    ActivationRule, EntryOrigin, FactBody, FactId, IdempotencyKey, KnowledgeChange, PlayerContribution, RetrievalHint,
    RumorBody, RumorId, StorySummary, Turn, TurnNumber, TurnSegment,
};

fn summary(text: &str, covered_through: u64) -> StorySummary {
    StorySummary {
        text: text.to_owned(),
        covered_through: TurnNumber::new(covered_through),
    }
}

fn turn(number: u64, segment: &str, status: TurnStatus) -> Turn {
    Turn {
        turn_number: TurnNumber::new(number),
        idempotency_key: IdempotencyKey::try_new(format!("key-{number}")).expect("valid idempotency key"),
        player_contribution: PlayerContribution {
            raw: format!("raw-{number}"),
            processed: format!("processed-{number}"),
        },
        turn_segment: TurnSegment::new(segment.to_owned()),
        knowledge_change: KnowledgeChange::default(),
        turn_evaluation: turn_evaluation(),
        turn_status: status,
    }
}

#[test]
fn blank_summary_is_treated_as_absent() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("  \n ", 1));

    assert!(story_summary(&story_ctx).is_none());
}

#[test]
fn summary_is_trimmed() {
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("  Earlier events.  ", 1));

    assert_eq!(story_summary(&story_ctx), Some("Earlier events."));
}

#[test]
fn turns_story_includes_pass_and_choice() {
    let mut story_ctx = StoryContext::new();
    story_ctx.recent_turns.push_back(turn(1, "The door creaks.", TurnStatus::Pass));
    story_ctx
        .recent_turns
        .push_back(turn(2, "  The hall is dark.  ", TurnStatus::Choice));
    story_ctx.recent_turns.push_back(turn(3, "  ", TurnStatus::Pass));

    assert_eq!(recent_story(&story_ctx), "The door creaks.\n\nThe hall is dark.");
}

#[tokio::test]
async fn opening_is_loaded_only_when_summary_is_absent() {
    let store = store_with_default_pack().await;
    let mut story_ctx = StoryContext::new();

    let opening = load_story_opening(PipelineStage::Baseline, &story_ctx, &store)
        .await
        .expect("opening loads");
    assert!(!opening.is_empty());

    story_ctx.summary = Some(summary("Earlier events.", 1));
    let opening = load_story_opening(PipelineStage::Baseline, &story_ctx, &store)
        .await
        .expect("opening skipped");
    assert!(opening.is_empty());
}

#[tokio::test]
async fn opening_is_loaded_when_summary_is_absent_even_with_recent_story() {
    let store = store_with_default_pack().await;
    let mut story_ctx = StoryContext::new();
    story_ctx.recent_turns.push_back(turn(1, "The door creaks.", TurnStatus::Pass));

    let opening = load_story_opening(PipelineStage::Plan, &story_ctx, &store)
        .await
        .expect("opening loads");

    assert!(!opening.is_empty());
}

#[tokio::test]
async fn opening_is_skipped_when_summary_is_present_even_without_recent_story() {
    let store = store_with_default_pack().await;
    let mut story_ctx = StoryContext::new();
    story_ctx.summary = Some(summary("Earlier events.", 1));

    let opening = load_story_opening(PipelineStage::Generate, &story_ctx, &store)
        .await
        .expect("opening skipped");

    assert!(opening.is_empty());
}

fn text(value: &str) -> BoundedText {
    BoundedText::try_new(value, "test", usize::MAX).expect("test text should fit")
}

#[test]
fn story_constraints_render_as_trimmed_bullets_and_skip_blank_items() {
    let constraints = [
        text("  The ritual truth is revealed. "),
        text("  "),
        text("Modern technology appears."),
    ];

    assert_eq!(
        story_constraints(&constraints),
        "- The ritual truth is revealed.\n- Modern technology appears."
    );
}

#[test]
fn narrative_directions_render_dramatic_focus_as_bullets() {
    let directions = [NarrativeDirection {
        node_id: crate::core::NarrativeNodeId::try_new("arrival").expect("test node id should be valid"),
        dramatic_focus: text("The traveler reaches the grey forest."),
    }];

    assert_eq!(narrative_directions(&directions), "- The traveler reaches the grey forest.");
}

fn fact(id: &str, content: &str) -> FactEntry {
    FactEntry {
        id: FactId::try_new(id).expect("test fact id should be valid"),
        origin: EntryOrigin::Seed,
        body: FactBody {
            content: text(content),
            activation: ActivationRule::default(),
            retrieval: RetrievalHint::try_new("hint").expect("test hint should be valid"),
        },
    }
}

fn rumor(id: &str, content: &str) -> RumorEntry {
    RumorEntry {
        id: RumorId::try_new(id).expect("test rumor id should be valid"),
        origin: EntryOrigin::Seed,
        body: RumorBody {
            content: text(content),
            activation: ActivationRule::default(),
            retrieval: RetrievalHint::try_new("hint").expect("test hint should be valid"),
        },
    }
}

#[test]
fn known_facts_render_only_content_as_bullets() {
    let facts = [
        fact("keeper_name", "  The forest keeper is named Ash. "),
        fact("blank", "  "),
        fact("river", "The river runs north."),
    ];

    assert_eq!(
        known_facts(&facts),
        "- The forest keeper is named Ash.\n- The river runs north."
    );
}

#[test]
fn rumors_render_only_content_as_bullets() {
    let rumors_list = [rumor("ghost", "A ghost haunts the mill.")];

    assert_eq!(rumors(&rumors_list), "- A ghost haunts the mill.");
}

#[test]
fn retrieved_knowledge_is_rendered_after_activated_knowledge() {
    let activated_facts = [fact("keeper", "The keeper is named Ash.")];
    let retrieved_facts = [fact("river", "The river runs north.")];
    let activated_rumors = [rumor("ghost", "A ghost haunts the mill.")];
    let retrieved_rumors = [rumor("bell", "The chapel bell rings by itself.")];

    assert_eq!(
        known_facts_with_retrieved(&activated_facts, &retrieved_facts),
        "- The keeper is named Ash.\n- The river runs north."
    );
    assert_eq!(
        rumors_with_retrieved(&activated_rumors, &retrieved_rumors),
        "- A ghost haunts the mill.\n- The chapel bell rings by itself."
    );
}

#[test]
fn empty_retrieved_knowledge_preserves_activated_only_rendering() {
    let facts = [fact("keeper", "The keeper is named Ash.")];
    let rumors_list = [rumor("ghost", "A ghost haunts the mill.")];

    assert_eq!(known_facts_with_retrieved(&facts, &[]), known_facts(&facts));
    assert_eq!(rumors_with_retrieved(&rumors_list, &[]), rumors(&rumors_list));
}

#[test]
fn empty_lists_render_as_empty_text() {
    assert_eq!(story_constraints(&[]), "");
    assert_eq!(narrative_directions(&[]), "");
    assert_eq!(known_facts(&[]), "");
    assert_eq!(rumors(&[]), "");
}
