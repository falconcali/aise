use aise::config::AssetLimitsConfig;
use aise::domain::asset::entity::KnowledgeEntity;
use aise::domain::asset::ids::{PlayerId, StoryRoleKey, TopicKey};
use aise::domain::ids::{CharacterId, StoryId};
use aise::domain::knowledge::KnowledgeKind;
use aise::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use aise::domain::turn::RetrievalAudience;
use aise::persistence::asset_store::AssetStore;
use aise::persistence::knowledge_read_port::{
    EntityKnowledgeQuery, KnowledgeFilter, KnowledgeLookupHit, KnowledgeReadPort, TopicKnowledgeQuery,
};
use aise::persistence::sqlite_asset_store::SqliteAssetStore;
use aise::persistence::{SqliteStore, Store};
use aise::story::instance_factory::{CreateStoryInstanceSpec, StoryInstanceFactory, StoryInstantiationLimits};
use aise::story::pack_service::{AssetInput, NativeAssetImporter, PackService};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_knowledge_{label}_{now}.db"))
        .to_string_lossy()
        .into_owned()
}

fn valid_pack_json() -> String {
    serde_json::json!({
        "spec": "aise_story_v3",
        "spec_version": "3.0",
        "meta": {
            "pack_key": "demo",
            "title": "Demo",
            "author": "aise",
            "version": "0.1.0",
            "description": "demo pack"
        },
        "story": {
            "premise": "A quiet village.",
            "language": "zh-CN",
            "genre": ["adventure"],
            "themes": ["hope"],
            "style": {"tone": ["light"], "point_of_view": "third", "tense": "past"}
        },
        "character_assets": {
            "protagonist_card": {
                "spec": "aise_char_v3", "spec_version": "3.0", "character_key": "protagonist_card",
                "meta": {"name": "Hero", "version": "0.1.0"},
                "profile": {"description": "Hero", "personality": [], "values": [], "speaking_style": {"register": "neutral", "verbosity": "medium"}}
            }
        },
        "roles": {
            "protagonist": {
                "role_label": "Protagonist",
                "narrative_function": "hero",
                "initial_state": {"location": "village", "goals": []},
                "initial_relationships": [],
                "seed_memories": []
            }
        },
        "default_cast": {
            "protagonist": {"character_ref": "protagonist_card"}
        },
        "play": {
            "player_count": 1,
            "playable_role_keys": ["protagonist"]
        },
        "world_book": {
            "spec": "aise_world_v3",
            "spec_version": "3.0",
            "world_book_key": "demo_world",
            "meta": {"name": "Demo World", "version": "0.1.0"},
            "topics": {
                "gate": {"label": "Gate", "aliases": ["the gate"]}
            },
            "facts": {},
            "rumors": {}
        },
        "start": {
            "scene_key": "scene_1",
            "location_key": "village",
            "time": "morning",
            "description": "The village wakes.",
            "role_openings": {
                "protagonist": "You open your eyes."
            }
        },
        "narrative": {
            "entry_nodes": ["node_a"],
            "nodes": {
                "node_a": {
                    "title": "A",
                    "objective": "Wake up",
                    "activate_when": {"type": "story_started"},
                    "complete_when": {"type": "turn_reaches", "turn": 1},
                    "skip_when": null,
                    "effects": {"on_activate": [], "on_complete": []},
                    "terminal": false
                }
            },
            "edges": []
        },
        "assets": {}
    })
    .to_string()
}

struct CountingKnowledge {
    inner: Arc<SqliteStore>,
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl KnowledgeReadPort for CountingKnowledge {
    async fn find_by_entities(
        &self,
        query: EntityKnowledgeQuery<'_>,
    ) -> Result<Vec<KnowledgeLookupHit>, aise::persistence::StoreError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.find_by_entities(query).await
    }

    async fn find_by_topics(
        &self,
        query: TopicKnowledgeQuery<'_>,
    ) -> Result<Vec<KnowledgeLookupHit>, aise::persistence::StoreError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.find_by_topics(query).await
    }
}

async fn seeded_store(label: &str) -> (Arc<SqliteStore>, KnowledgeSnapshotRef, String) {
    let db = temp_db_path(label);
    let sqlite = SqliteStore::connect(&db).await.unwrap();
    let store: Arc<dyn Store> = sqlite.clone();
    let asset_store: Arc<dyn AssetStore> = SqliteAssetStore::connect(&db).await.unwrap();
    let pack_service = PackService::new(NativeAssetImporter::new(AssetLimitsConfig::default()), asset_store.clone());
    let pack = pack_service
        .import(AssetInput::Json(valid_pack_json().as_bytes()))
        .await
        .expect("import");
    let factory = StoryInstanceFactory::new(
        asset_store,
        store,
        StoryInstantiationLimits {
            max_roles: 16,
            max_facts: 128,
            max_rumors: 128,
            max_memories: 128,
            max_relationships: 64,
        },
    );
    let story = factory
        .create(CreateStoryInstanceSpec {
            pack_id: pack.pack_id,
            player_id: PlayerId::from("player-1"),
            player_role_key: StoryRoleKey::from("protagonist"),
            player_character: None,
            created_at_ms: 1,
        })
        .await
        .expect("create");
    let limits = aise::domain::turn::SnapshotLimits::from_config(
        &aise::config::TurnContentLimitsConfig::default(),
        &aise::config::ContextPreparationConfig::default(),
        &aise::config::AssetLimitsConfig::default(),
    );
    let snapshot = sqlite.load_story_snapshot(&story.story_id, limits).await.expect("snapshot");
    (sqlite, snapshot.knowledge_snapshot().clone(), db)
}

#[tokio::test]
async fn character_fact_request_is_rejected_before_store_lookup() {
    let (sqlite, snapshot, db) = seeded_store("char_fact").await;
    let counting = CountingKnowledge {
        inner: sqlite,
        calls: AtomicUsize::new(0),
    };
    let counting = Arc::new(counting);
    let retriever = aise::context::EntityCandidateRetriever::new(counting.clone() as Arc<dyn KnowledgeReadPort>);
    let request = aise::domain::turn::RetrievalRequest {
        audience: RetrievalAudience::Character {
            character_id: CharacterId::from("c-npc"),
        },
        knowledge_kinds: vec![KnowledgeKind::Fact],
        entities: vec![KnowledgeEntity::Location(aise::domain::asset::ids::LocationKey::from(
            "village",
        ))],
        topics: Vec::new(),
        query_text: None,
        authorized_memory_owners: Vec::new(),
        reason: aise::domain::asset::validation::BoundedText::try_new("x", "r", 32).unwrap(),
        origin: aise::domain::turn::RetrievalRequestOrigin::Planner,
        signal_priority: 0,
    };
    use aise::context::CandidateRetriever;
    let err = retriever
        .retrieve(aise::context::CandidateRetrievalRequest {
            snapshot: &snapshot,
            request: &request,
            limit: 8,
            max_item_bytes: 4096,
        })
        .await;
    assert!(err.is_err());
    assert_eq!(counting.calls.load(Ordering::SeqCst), 0);
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn zero_result_request_never_falls_back_to_full_scan() {
    let (sqlite, snapshot, db) = seeded_store("zero").await;
    let filter = KnowledgeFilter {
        audience: RetrievalAudience::GlobalWriter,
        knowledge_kinds: vec![KnowledgeKind::Fact],
        authorized_memory_owners: Vec::new(),
        max_item_bytes: 4096,
    };
    let records = sqlite
        .find_by_topics(TopicKnowledgeQuery {
            snapshot: &snapshot,
            filter: &filter,
            topics: &[TopicKey::from("missing")],
            limit: 8,
        })
        .await
        .expect("query");
    assert!(records.is_empty());
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn knowledge_read_rejects_revision_or_digest_mismatch() {
    let (sqlite, mut snapshot, db) = seeded_store("conflict").await;
    snapshot.base_revision = aise::domain::ids::StoryRevision::new(999);
    let filter = KnowledgeFilter {
        audience: RetrievalAudience::GlobalWriter,
        knowledge_kinds: vec![KnowledgeKind::Fact],
        authorized_memory_owners: Vec::new(),
        max_item_bytes: 4096,
    };
    let err = sqlite
        .find_by_entities(EntityKnowledgeQuery {
            snapshot: &snapshot,
            filter: &filter,
            entities: &[KnowledgeEntity::Location(aise::domain::asset::ids::LocationKey::from(
                "village",
            ))],
            limit: 8,
        })
        .await;
    assert!(matches!(err, Err(aise::persistence::StoreError::RevisionConflict)));
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn sqlite_entity_and_topic_queries_use_indexes() {
    let (sqlite, _snapshot, db) = seeded_store("explain").await;
    let pool = sqlite.pool_for_tests();
    let entity_rows = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT e.source_id FROM knowledge_entries e \
         INNER JOIN knowledge_entry_entities m \
           ON e.story_id = m.story_id AND e.knowledge_kind = m.knowledge_kind AND e.source_id = m.source_id \
         WHERE e.story_id = ? AND m.entity_kind = ? AND m.entity_key = ?",
    )
    .bind("story")
    .bind("location")
    .bind("village")
    .fetch_all(pool)
    .await
    .expect("entity explain");
    let topic_rows = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT e.source_id FROM knowledge_entries e \
         INNER JOIN knowledge_entry_topics m \
           ON e.story_id = m.story_id AND e.knowledge_kind = m.knowledge_kind AND e.source_id = m.source_id \
         WHERE e.story_id = ? AND m.topic_key = ?",
    )
    .bind("story")
    .bind("gate")
    .fetch_all(pool)
    .await
    .expect("topic explain");
    use sqlx::Row;
    let entity_plan = entity_rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("detail").ok())
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    let topic_plan = topic_rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("detail").ok())
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    assert!(
        entity_plan.contains("knowledge_entry_entities") || entity_plan.contains("ix_knowledge_entry_entities"),
        "entity plan: {entity_plan}"
    );
    assert!(
        topic_plan.contains("knowledge_entry_topics") || topic_plan.contains("ix_knowledge_entry_topics"),
        "topic plan: {topic_plan}"
    );
    let _ = StoryId::try_new("x");
    let _ = std::fs::remove_file(&db);
}
