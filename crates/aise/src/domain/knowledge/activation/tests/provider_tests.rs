use super::*;
use crate::domain::asset::ids::Sha256Digest;
use crate::domain::ids::{StoryId, StoryRevision};
use crate::domain::knowledge::KnowledgeIdHighWater;

fn assert_object_safe(_: Option<&dyn ActivationSeedProvider>) {}

#[test]
fn request_can_be_constructed_and_trait_is_object_safe() {
    let snapshot = KnowledgeSnapshotRef {
        story_id: StoryId::try_new("story-provider").unwrap(),
        pack_digest: Sha256Digest::from_bytes([7u8; 32]),
        base_revision: StoryRevision::new(1),
        knowledge_id_high_water: KnowledgeIdHighWater::zero(),
    };
    let scan_buffer = ActivationScanBuffer::try_new(Vec::new(), 1, 1).unwrap();
    let delivery = KnowledgeDelivery::Writer;
    let request = ActivationSeedRequest {
        knowledge_snapshot: &snapshot,
        scan_buffer: &scan_buffer,
        query_text: None,
        allowed_kinds: &[],
        delivery: &delivery,
        limit: 1,
    };
    assert_eq!(request.limit, 1);
    assert_object_safe(None);
}
