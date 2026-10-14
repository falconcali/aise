use super::*;
use crate::core::EntryOrigin;
use crate::core::tests::snake_pack;

fn pipeline() -> RetrievalPipeline {
    RetrievalPipeline::new(Arc::new(Store::default()))
}

fn fact_id(value: &str) -> FactId {
    FactId::try_new(value).expect("valid fact id")
}

fn rumor_id(value: &str) -> RumorId {
    RumorId::try_new(value).expect("valid rumor id")
}

#[test]
fn fact_only_input_resolves_requested_fact() {
    let story_pack = snake_pack();
    let fact_ids = [fact_id("bai_is_thousand_year_snake")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &[],
    };

    let output = pipeline().resolve(&story_pack, &input).expect("fact resolves");

    assert_eq!(output.facts.len(), 1);
    assert_eq!(output.facts[0].id, fact_ids[0]);
    assert!(matches!(output.facts[0].origin, EntryOrigin::Seed));
    assert_eq!(output.facts[0].body.content, story_pack.seed_facts[&fact_ids[0]].content);
    assert!(output.rumors.is_empty());
}

#[test]
fn rumor_only_input_resolves_requested_rumor() {
    let story_pack = snake_pack();
    let rumor_ids = [rumor_id("rumor_white_dress_woman")];
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &rumor_ids,
    };

    let output = pipeline().resolve(&story_pack, &input).expect("rumor resolves");

    assert!(output.facts.is_empty());
    assert_eq!(output.rumors.len(), 1);
    assert_eq!(output.rumors[0].id, rumor_ids[0]);
    assert!(matches!(output.rumors[0].origin, EntryOrigin::Seed));
    assert_eq!(output.rumors[0].body.content, story_pack.seed_rumors[&rumor_ids[0]].content);
}

#[test]
fn mixed_input_resolves_both_knowledge_kinds() {
    let story_pack = snake_pack();
    let fact_ids = [fact_id("bai_is_thousand_year_snake")];
    let rumor_ids = [rumor_id("rumor_white_dress_woman")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &rumor_ids,
    };

    let output = pipeline().resolve(&story_pack, &input).expect("mixed input resolves");

    assert_eq!(output.facts.len(), 1);
    assert_eq!(output.rumors.len(), 1);
}

#[test]
fn empty_input_resolves_empty_output() {
    let story_pack = snake_pack();
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &[],
    };

    let output = pipeline().resolve(&story_pack, &input).expect("empty input resolves");

    assert!(output.facts.is_empty());
    assert!(output.rumors.is_empty());
}

#[test]
fn retrieval_resolves_selected_entries_in_request_order() {
    let story_pack = snake_pack();
    let fact_ids = [
        fact_id("west_lake_has_broken_bridge"),
        fact_id("bai_is_thousand_year_snake"),
    ];
    let rumor_ids = [
        rumor_id("rumor_bai_marriage_is_omen"),
        rumor_id("rumor_white_dress_woman"),
    ];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &rumor_ids,
    };

    let output = pipeline().resolve(&story_pack, &input).expect("ordered input resolves");

    assert_eq!(
        output.facts.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
        fact_ids.iter().collect::<Vec<_>>()
    );
    assert_eq!(
        output.rumors.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
        rumor_ids.iter().collect::<Vec<_>>()
    );
}

#[test]
fn retrieval_rejects_missing_fact_id() {
    let story_pack = snake_pack();
    let fact_ids = [fact_id("missing_fact")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &[],
    };

    let error = pipeline().resolve(&story_pack, &input).err().expect("missing fact fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Retrieval,
            code: "retrieval_fact_missing",
            ..
        }
    ));
}

#[test]
fn retrieval_rejects_missing_rumor_id() {
    let story_pack = snake_pack();
    let rumor_ids = [rumor_id("missing_rumor")];
    let input = RetrievalInput {
        fact_ids: &[],
        rumor_ids: &rumor_ids,
    };

    let error = pipeline().resolve(&story_pack, &input).err().expect("missing rumor fails");

    assert!(matches!(
        error,
        PipelineError::InconsistentData {
            stage: PipelineStage::Retrieval,
            code: "retrieval_rumor_missing",
            ..
        }
    ));
}

#[test]
fn mixed_input_with_missing_id_returns_no_partial_output() {
    let story_pack = snake_pack();
    let fact_ids = [fact_id("bai_is_thousand_year_snake")];
    let rumor_ids = [rumor_id("missing_rumor")];
    let input = RetrievalInput {
        fact_ids: &fact_ids,
        rumor_ids: &rumor_ids,
    };

    let result = pipeline().resolve(&story_pack, &input);

    assert!(result.is_err());
}
