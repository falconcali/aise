use super::*;

fn plan_output(facts_to_retrieve: Vec<FactId>, rumors_to_retrieve: Vec<RumorId>) -> PlanOutput {
    PlanOutput {
        story_goal: BoundedText::default(),
        facts_to_retrieve,
        rumors_to_retrieve,
    }
}

#[test]
fn empty_knowledge_selection_does_not_require_retrieval() {
    let output = plan_output(Vec::new(), Vec::new());

    assert!(!output.requires_retrieval());
}

#[test]
fn fact_selection_requires_retrieval() {
    let output = plan_output(vec![FactId::try_new("fact-1").expect("valid fact id")], Vec::new());

    assert!(output.requires_retrieval());
}

#[test]
fn rumor_selection_requires_retrieval() {
    let output = plan_output(Vec::new(), vec![RumorId::try_new("rumor-1").expect("valid rumor id")]);

    assert!(output.requires_retrieval());
}

#[test]
fn character_thinking_remains_disabled() {
    let output = plan_output(Vec::new(), Vec::new());

    assert!(!output.requires_character_thinking());
}
