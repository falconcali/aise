use super::*;
use crate::core::tests::{snake_pack, xu_xian_card};
use crate::core::{
    BoundedText, EntryOrigin, IdempotencyKey, MAX_OPENING_BYTES, MAX_ROLES_PER_STORY, PackRef, StoryCommit,
    StoryContext, StoryError, StoryInstanceSpec, StoryPackSnapshot,
};
use crate::persistence::{PackStore, PersistenceError, StoryStore};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

struct MissingStoryStore;

#[async_trait]
impl StoryStore for MissingStoryStore {
    async fn create(&self, _spec: StoryInstanceSpec) -> Result<StoryInstanceInfo, PersistenceError> {
        Err(PersistenceError::Unavailable {
            message: "create unavailable".to_owned(),
        })
    }

    async fn remove(&self, _story_id: &StoryId) -> Result<(), PersistenceError> {
        Err(PersistenceError::NotFound)
    }

    async fn get_info(&self, _story_id: &StoryId) -> Result<StoryInstanceInfo, PersistenceError> {
        Err(PersistenceError::NotFound)
    }

    async fn load(&self, _story_id: &StoryId) -> Result<StoryContext, PersistenceError> {
        Err(PersistenceError::NotFound)
    }

    async fn find_committed(
        &self,
        _story_id: &StoryId,
        _idempotency_key: &IdempotencyKey,
    ) -> Result<Option<StoryCommit>, PersistenceError> {
        Ok(None)
    }

    async fn commit(&self, _commit: StoryCommit) -> Result<StoryCommit, PersistenceError> {
        Err(PersistenceError::Unavailable {
            message: "commit unavailable".to_owned(),
        })
    }
}

struct RetryingPackStore {
    release_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl PackStore for RetryingPackStore {
    async fn retain(&self, _story_id: &StoryId, _snapshot: StoryPackSnapshot) -> Result<(), PersistenceError> {
        Ok(())
    }

    async fn release(&self, _story_id: &StoryId) -> Result<(), PersistenceError> {
        if self.release_calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(PersistenceError::Unavailable {
                message: "release unavailable".to_owned(),
            });
        }
        Ok(())
    }

    async fn get_pack(&self, _pack_ref: &PackRef) -> Result<Arc<StoryPack>, PersistenceError> {
        Err(PersistenceError::NotFound)
    }
}

fn role_id(value: &str) -> RoleId {
    RoleId::try_new(value).expect("test role id should be valid")
}

fn snake_pack_ref() -> PackRef {
    snake_pack().pack_ref().expect("pack ref should compute")
}

fn creation_spec(cast: BTreeMap<RoleId, RoleActorSpec>, player_role: &str) -> StoryCreationSpec {
    StoryCreationSpec {
        story_pack: snake_pack(),
        cast,
        player_id: PlayerId::try_new("player").expect("test player id should be valid"),
        player_role: role_id(player_role),
    }
}

async fn create_story(store: &Arc<Store>, spec: StoryCreationSpec) -> Result<StoryInstanceInfo, EngineError> {
    AiseFactory.create_story(Arc::clone(store), spec).await
}

#[tokio::test]
async fn created_story_pins_a_frozen_pack_snapshot() {
    let store = AiseFactory.create_store(PersistanceConfig::default());

    let info = create_story(&store, creation_spec(BTreeMap::new(), "xu_xian"))
        .await
        .expect("story should be created");

    let story_ctx = store.story_store().load(&info.story_id).await.expect("story should load");
    assert_eq!(info.player_id.as_str(), "player");
    assert_eq!(story_ctx.player, role_id("xu_xian"));
    assert_eq!(story_ctx.opening.as_str(), snake_pack().opening.as_str());
    assert_eq!(story_ctx.pack_ref.pack_id, snake_pack().pack_id);
    assert_eq!(story_ctx.pack_ref.version, snake_pack_ref().version);
    assert_eq!(story_ctx.pack_ref.digest, snake_pack_ref().digest);
    let pack = store
        .pack_store()
        .get_pack(&story_ctx.pack_ref)
        .await
        .expect("pack snapshot should load");
    assert_eq!(pack.opening, snake_pack().opening);
    assert_eq!(story_ctx.facts.len(), pack.seed_facts.len());
    for (id, body) in &pack.seed_facts {
        let entry = &story_ctx.facts[id];
        assert_eq!(entry.id, *id);
        assert!(matches!(entry.origin, EntryOrigin::Seed));
        assert_eq!(entry.body.content, body.content);
        assert_eq!(entry.body.retrieval.as_str(), body.retrieval.as_str());
    }
    assert_eq!(story_ctx.rumors.len(), pack.seed_rumors.len());
    for (id, body) in &pack.seed_rumors {
        let entry = &story_ctx.rumors[id];
        assert_eq!(entry.id, *id);
        assert!(matches!(entry.origin, EntryOrigin::Seed));
        assert_eq!(entry.body.content, body.content);
        assert_eq!(entry.body.retrieval.as_str(), body.retrieval.as_str());
    }
    assert_eq!(story_ctx.roles.len(), pack.roles.len());
    for (id, body) in &pack.roles {
        let role_entry = &story_ctx.roles[id];
        assert_eq!(role_entry.id, *id);
        assert!(matches!(role_entry.origin, EntryOrigin::Seed));
        assert_eq!(
            serde_json::to_value(&role_entry.spec).expect("runtime role spec should serialize"),
            serde_json::to_value(&body.spec).expect("seed role spec should serialize")
        );
        assert_eq!(
            serde_json::to_value(&role_entry.profile).expect("runtime profile should serialize"),
            serde_json::to_value(&body.default_profile).expect("default profile should serialize")
        );
        assert_eq!(role_entry.memories.len(), body.seed_memories.len());
        for (mem_id, mem_body) in &body.seed_memories {
            let mem_entry = &role_entry.memories[mem_id];
            assert_eq!(mem_entry.id, *mem_id);
            assert!(matches!(mem_entry.origin, EntryOrigin::Seed));
            assert_eq!(mem_entry.body.content, mem_body.content);
            assert_eq!(mem_entry.body.retrieval.as_str(), mem_body.retrieval.as_str());
        }
    }
}

#[tokio::test]
async fn character_card_overrides_only_the_runtime_profile() {
    let store = AiseFactory.create_store(PersistanceConfig::default());
    let pack = snake_pack();
    let seed = pack.roles[&role_id("xu_xian")].clone();
    let card = xu_xian_card();
    let expected_profile = serde_json::to_value(&card.profile).expect("card profile should serialize");
    let cast = BTreeMap::from([(role_id("xu_xian"), RoleActorSpec::Card(Box::new(card)))]);

    let info = create_story(
        &store,
        StoryCreationSpec {
            story_pack: pack,
            cast,
            player_id: PlayerId::try_new("player").expect("test player id should be valid"),
            player_role: role_id("xu_xian"),
        },
    )
    .await
    .expect("story should be created");

    let story_ctx = store.story_store().load(&info.story_id).await.expect("story should load");
    let runtime = &story_ctx.roles[&role_id("xu_xian")];
    assert_eq!(
        serde_json::to_value(&runtime.profile).expect("runtime profile should serialize"),
        expected_profile
    );
    assert_eq!(
        serde_json::to_value(&runtime.spec).expect("runtime role spec should serialize"),
        serde_json::to_value(&seed.spec).expect("seed role spec should serialize")
    );
    assert_eq!(
        runtime.memories.keys().collect::<Vec<_>>(),
        seed.seed_memories.keys().collect::<Vec<_>>()
    );
    for (memory_id, seed_body) in &seed.seed_memories {
        let runtime_memory = &runtime.memories[memory_id];
        assert_eq!(
            serde_json::to_value(&runtime_memory.body).expect("runtime memory should serialize"),
            serde_json::to_value(seed_body).expect("seed memory should serialize")
        );
        assert!(matches!(runtime_memory.origin, EntryOrigin::Seed));
    }
}

#[tokio::test]
async fn non_playable_player_role_is_rejected_without_retaining_assets() {
    let store = AiseFactory.create_store(PersistanceConfig::default());

    let error = create_story(&store, creation_spec(BTreeMap::new(), "bai_suzhen"))
        .await
        .expect_err("non playable role should fail");

    assert!(matches!(
        error,
        EngineError::Story(StoryError::RoleNotPlayable { role_id }) if role_id == "bai_suzhen"
    ));
    assert!(matches!(
        store.pack_store().get_pack(&snake_pack_ref()).await,
        Err(PersistenceError::NotFound)
    ));
}

#[tokio::test]
async fn invalid_role_assets_are_rejected_without_retaining_assets() {
    let store = AiseFactory.create_store(PersistanceConfig::default());
    let mut spec = creation_spec(BTreeMap::new(), "xu_xian");
    let role = spec.story_pack.roles.values().next().expect("fixture role").clone();
    spec.story_pack.roles = (0..MAX_ROLES_PER_STORY)
        .map(|index| (role_id(&format!("role-{index:03}")), role.clone()))
        .collect();
    spec.story_pack.roles.insert(role_id("xu_xian"), role);
    let pack_ref = spec.story_pack.pack_ref().expect("pack ref should compute");

    let error = create_story(&store, spec)
        .await
        .expect_err("oversized role catalog should fail");

    assert!(matches!(error, EngineError::Story(StoryError::TooManyRoles { .. })));
    assert!(matches!(
        store.pack_store().get_pack(&pack_ref).await,
        Err(PersistenceError::NotFound)
    ));
}

#[tokio::test]
async fn opening_at_limit_is_frozen_into_story_context() {
    let store = AiseFactory.create_store(PersistanceConfig::default());
    let mut spec = creation_spec(BTreeMap::new(), "xu_xian");
    spec.story_pack.opening =
        BoundedText::try_new("x".repeat(MAX_OPENING_BYTES), "test", usize::MAX).expect("opening should fit");

    let info = create_story(&store, spec).await.expect("story should be created");
    let story_ctx = store.story_store().load(&info.story_id).await.expect("story should load");

    assert_eq!(story_ctx.opening.as_str().len(), MAX_OPENING_BYTES);
}

#[tokio::test]
async fn opening_over_limit_is_rejected_without_retaining_pack() {
    let store = AiseFactory.create_store(PersistanceConfig::default());
    let mut spec = creation_spec(BTreeMap::new(), "xu_xian");
    spec.story_pack.opening =
        BoundedText::try_new("x".repeat(MAX_OPENING_BYTES + 1), "test", usize::MAX).expect("fixture should build");
    let pack_ref = spec.story_pack.pack_ref().expect("pack ref should compute");

    let error = create_story(&store, spec).await.expect_err("oversized opening should fail");

    assert!(matches!(
        error,
        EngineError::Story(StoryError::OpeningTooLarge {
            actual,
            maximum: MAX_OPENING_BYTES
        }) if actual == MAX_OPENING_BYTES + 1
    ));
    assert!(matches!(
        store.pack_store().get_pack(&pack_ref).await,
        Err(PersistenceError::NotFound)
    ));
}

#[tokio::test]
async fn removing_story_releases_its_asset_snapshots() {
    let store = AiseFactory.create_store(PersistanceConfig::default());
    let info = create_story(&store, creation_spec(BTreeMap::new(), "xu_xian"))
        .await
        .expect("story should be created");

    AiseFactory
        .remove_story(Arc::clone(&store), &info.story_id)
        .await
        .expect("story should be removed");

    assert!(matches!(
        store.pack_store().get_pack(&snake_pack_ref()).await,
        Err(PersistenceError::NotFound)
    ));
}

#[tokio::test]
async fn factory_repeated_remove_retries_failed_pack_release() {
    let release_calls = Arc::new(AtomicUsize::new(0));
    let store = Arc::new(Store::from_backends(
        Box::new(MissingStoryStore),
        Box::new(RetryingPackStore {
            release_calls: Arc::clone(&release_calls),
        }),
    ));
    let target_story_id = StoryId::try_new("retry-story").expect("test story id should be valid");

    assert!(matches!(
        AiseFactory.remove_story(Arc::clone(&store), &target_story_id).await,
        Err(EngineError::Persistence(PersistenceError::Unavailable { .. }))
    ));
    AiseFactory
        .remove_story(store, &target_story_id)
        .await
        .expect("repeated removal should retry release");
    assert_eq!(release_calls.load(Ordering::SeqCst), 2);
}
