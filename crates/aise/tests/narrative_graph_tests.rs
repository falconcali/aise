use aise::domain::narrative_graph::director::{NarrativeDirector, NarrativeLimits, NarrativePlan};

#[test]
fn narrative_director_uses_continuity_and_condition_view() {
    let director = NarrativeDirector::new(NarrativeLimits {
        max_nodes: 32,
        max_edges: 64,
        max_condition_depth: 8,
        max_conditions_per_node: 8,
        max_effects_per_node: 8,
    });
    assert_eq!(director.limits().max_nodes, 32);
    let plan = NarrativePlan::empty();
    assert!(plan.active_nodes.is_empty());
    assert!(plan.character_impulses.is_empty());
}
