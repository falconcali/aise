use aise::config::{AssetLimitsConfig, NarrativeConfig};
use aise::domain::asset::ids::PlayerId;
use aise::domain::ids::RoleId;
use aise::domain::knowledge::{KnowledgeKind, KnowledgeSourceId};
use aise::domain::story_instance::snapshot::KnowledgeSnapshotRef;
use aise::domain::turn::KnowledgeDelivery;
use aise::persistence::asset_store::AssetStore;
use aise::persistence::knowledge_read_port::{
    KnowledgeFilter, KnowledgeIndexQuery, KnowledgeReadPort, OwnerMemoryQuery, SourceKnowledgeQuery,
};
use aise::persistence::sqlite_asset_store::SqliteAssetStore;
use aise::persistence::{SqliteStore, Store};
use aise::story::instance_factory::{CreateStoryInstanceSpec, StoryInstanceFactory, StoryInstantiationLimits};
use aise::story::pack_service::{AssetInput, NativeAssetImporter, PackService};
use std::sync::Arc;
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
        "spec": "aise_story_v5",
        "spec_version": "5.0",
        "meta": {
            "pack_key": "demo",
            "title": "Demo",
            "author": "aise",
            "version": "0.1.0",
            "description": "demo pack",
            "tags": [],
            "cover_asset": null
        },
        "story": {
            "language": "zh-CN",
            "genre": ["adventure"],
            "themes": ["hope"],
            "style": {"tone": ["light"], "point_of_view": "third", "tense": "past"}
        },
        "roles": {
            "protagonist": {
                "role_label": "Protagonist",
                "narrative_function": "hero",
                "default_profile": {
                    "name": "Hero",
                    "dialogue_examples": []
                },
                "background": null,
                "initial_state": {"location": "village", "goals": []},
                "initial_relationships": [],
                "seed_memories": [{
                    "memory_key": "private_arrival",
                    "kind": "personal",
                    "content": "I arrived before dawn.",
                    "salience": 70
                }]
            },
            "witness": {
                "role_label": "Witness",
                "narrative_function": "observer",
                "default_profile": {
                    "name": "Witness",
                    "dialogue_examples": []
                },
                "background": null,
                "initial_state": {"location": "village", "goals": []},
                "initial_relationships": [],
                "seed_memories": [{
                    "memory_key": "private_bell",
                    "kind": "observed",
                    "content": "I heard the bell at midnight.",
                    "salience": 60
                }]
            }
        },
        "play": {
            "player_count": 1,
            "playable_role_ids": ["protagonist"]
        },
        "world_book": {
            "spec": "aise_world_v5",
            "spec_version": "5.0",
            "world_book_key": "demo_world",
            "meta": {"name": "Demo World", "version": "0.1.0"},
            "facts": {
                "village_gate": {
                    "proposition": null,
                    "content": "The village gate is closed.",
                    "retrieval_hint": "Village gate status",
                    "salience": 80,
                    "activation": {
                        "match": {
                            "keys": ["Village gate status"],
                            "secondary_keys": [],
                            "scan_depth": null
                        },
                        "mode": {"enabled": true, "constant": false, "exact_target_only": false},
                        "recursion": {
                            "exclude_recursion": false,
                            "prevent_recursion": false,
                            "delay_until_recursion": null
                        },
                        "selection": {
                            "order": 0,
                            "probability": 100,
                            "groups": [],
                            "group_override": false,
                            "group_weight": 100,
                            "use_group_scoring": false
                        },
                        "timing": {"sticky_turns": 0, "cooldown_turns": 0, "delay_turns": 0},
                        "scope": {"generation_triggers": []},
                        "budget_class": "normal"
                    }
                }
            },
            "rumors": {
                "midnight_bell": {
                    "claim": null,
                    "content": "The bell rings by itself at midnight.",
                    "retrieval_hint": "Midnight bell rumor",
                    "salience": 50,
                    "activation": {
                        "match": {
                            "keys": ["Midnight bell rumor"],
                            "secondary_keys": [],
                            "scan_depth": null
                        },
                        "mode": {"enabled": true, "constant": false, "exact_target_only": false},
                        "recursion": {
                            "exclude_recursion": false,
                            "prevent_recursion": false,
                            "delay_until_recursion": null
                        },
                        "selection": {
                            "order": 0,
                            "probability": 100,
                            "groups": [],
                            "group_override": false,
                            "group_weight": 100,
                            "use_group_scoring": false
                        },
                        "timing": {"sticky_turns": 0, "cooldown_turns": 0, "delay_turns": 0},
                        "scope": {"generation_triggers": []},
                        "budget_class": "normal"
                    }
                }
            }
        },
        "start": {
            "scene_key": "scene_1",
            "location_key": "village",
            "time": "morning",
            "description": "The village wakes.",
            "opening": "You open your eyes."
        },
        "narrative": {
            "entry_nodes": ["node_a"],
            "nodes": {
                "node_a": {
                    "title": "A",
                    "dramatic_focus": "Wake up",
                    "activate_when": {"type": "story_started"},
                    "complete_when": {"type": "turn_reaches", "turn": 1},
                    "skip_when": null,
                    "effects": {"on_activate": [], "on_complete": []},
                    "terminal": false
                }
            },
            "edges": []
        },
        "constraints": {},
        "assets": {}
    })
    .to_string()
}

async fn seeded_store(label: &str) -> (Arc<SqliteStore>, KnowledgeSnapshotRef, String) {
    let db = temp_db_path(label);
    let sqlite = SqliteStore::connect(&db).await.unwrap();
    let store: Arc<dyn Store> = sqlite.clone();
    let asset_store: Arc<dyn AssetStore> = SqliteAssetStore::connect(&db).await.unwrap();
    let pack_service = PackService::new(
        NativeAssetImporter::new(
            AssetLimitsConfig::default(),
            NarrativeConfig::default(),
            aise::turn::turn_budget::activation_rule_limits(&aise::config::ActivationConfig::default()),
        ),
        asset_store.clone(),
    );
    let pack = pack_service
        .import(AssetInput::Json(valid_pack_json().as_bytes()))
        .await
        .expect("import");
    let factory = StoryInstanceFactory::new(
        asset_store,
        store,
        StoryInstantiationLimits {
            max_roles: 16,
            max_role_bytes: 131_072,
            max_facts: 128,
            max_rumors: 128,
            max_memories: 128,
            max_relationships: 64,
            max_opening_bytes: 8192,
            activation_rule_limits: aise::turn::turn_budget::activation_rule_limits(
                &aise::config::ActivationConfig::default(),
            ),
        },
        aise::turn::turn_budget::narrative_limits(&NarrativeConfig::default()),
    );
    let story = factory
        .create(CreateStoryInstanceSpec {
            pack_id: pack.pack_id,
            player_id: PlayerId::from("player-1"),
            player_role_id: RoleId::try_new("protagonist").unwrap(),
            role_profile_selections: std::collections::BTreeMap::new(),
            created_at_ms: 1,
        })
        .await
        .expect("create");
    let limits = aise::domain::turn::SnapshotLimits::from_config(
        &aise::config::TurnContentLimitsConfig::default(),
        &aise::config::ContextPreparationConfig::default(),
        &aise::config::AssetLimitsConfig::default(),
        &NarrativeConfig::default(),
    );
    let snapshot = sqlite.load_story_snapshot(&story.story_id, limits).await.expect("snapshot");
    (sqlite, snapshot.knowledge_snapshot().clone(), db)
}

#[tokio::test]
async fn source_id_lookup_returns_only_requested_records_in_stable_order() {
    let (sqlite, snapshot, db) = seeded_store("source_ids").await;
    let index = sqlite
        .list_index(KnowledgeIndexQuery {
            snapshot: &snapshot,
            knowledge_kinds: &[KnowledgeKind::Fact, KnowledgeKind::Rumor],
            limit: 16,
        })
        .await
        .expect("index");
    let requested = index.iter().rev().map(|record| record.source_id.clone()).collect::<Vec<_>>();
    let filter = KnowledgeFilter {
        delivery: KnowledgeDelivery::Writer,
        knowledge_kinds: vec![KnowledgeKind::Fact, KnowledgeKind::Rumor],
        max_item_bytes: 4096,
    };
    let records = sqlite
        .find_by_source_ids(SourceKnowledgeQuery {
            snapshot: &snapshot,
            filter: &filter,
            source_ids: &requested,
            limit: 16,
        })
        .await
        .expect("exact lookup");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].kind, KnowledgeKind::Fact);
    assert_eq!(records[0].content.as_str(), "The village gate is closed.");
    assert_eq!(records[1].kind, KnowledgeKind::Rumor);
    assert_eq!(records[1].content.as_str(), "The bell rings by itself at midnight.");
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn source_id_lookup_enforces_delivery_authorization_before_limit() {
    let (sqlite, snapshot, db) = seeded_store("authorization").await;
    let protagonist = RoleId::try_new("protagonist").unwrap();
    let witness = RoleId::try_new("witness").unwrap();
    let protagonist_memories = sqlite
        .find_memories_by_owner(OwnerMemoryQuery {
            snapshot: &snapshot,
            owner: &protagonist,
            limit: 8,
            max_item_bytes: 4096,
        })
        .await
        .expect("protagonist memories");
    let witness_memories = sqlite
        .find_memories_by_owner(OwnerMemoryQuery {
            snapshot: &snapshot,
            owner: &witness,
            limit: 8,
            max_item_bytes: 4096,
        })
        .await
        .expect("witness memories");
    let memory_ids = vec![
        protagonist_memories[0].source_id.clone(),
        witness_memories[0].source_id.clone(),
    ];
    let character_memory_filter = KnowledgeFilter {
        delivery: KnowledgeDelivery::Character {
            role_id: witness.clone(),
        },
        knowledge_kinds: vec![KnowledgeKind::Memory],
        max_item_bytes: 4096,
    };
    let authorized = sqlite
        .find_by_source_ids(SourceKnowledgeQuery {
            snapshot: &snapshot,
            filter: &character_memory_filter,
            source_ids: &memory_ids,
            limit: 1,
        })
        .await
        .expect("owner-filtered memories");
    assert_eq!(authorized.len(), 1);
    assert_eq!(authorized[0].memory_owner.as_ref(), Some(&witness));

    let character_fact_filter = KnowledgeFilter {
        delivery: KnowledgeDelivery::Character { role_id: witness },
        knowledge_kinds: vec![KnowledgeKind::Fact],
        max_item_bytes: 4096,
    };
    let fact_error = sqlite
        .find_by_source_ids(SourceKnowledgeQuery {
            snapshot: &snapshot,
            filter: &character_fact_filter,
            source_ids: &[],
            limit: 1,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        fact_error,
        aise::persistence::StoreError::ConstraintViolation { constraint }
            if constraint == "fact_forbidden_for_character_delivery"
    ));

    let writer_memory_filter = KnowledgeFilter {
        delivery: KnowledgeDelivery::Writer,
        knowledge_kinds: vec![KnowledgeKind::Memory],
        max_item_bytes: 4096,
    };
    let memory_error = sqlite
        .find_by_source_ids(SourceKnowledgeQuery {
            snapshot: &snapshot,
            filter: &writer_memory_filter,
            source_ids: &memory_ids,
            limit: 1,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        memory_error,
        aise::persistence::StoreError::ConstraintViolation { constraint }
            if constraint == "memory_forbidden_for_writer_delivery"
    ));
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn source_id_lookup_allows_rumor_for_writer_and_character() {
    let (sqlite, snapshot, db) = seeded_store("rumor_visibility").await;
    let index = sqlite
        .list_index(KnowledgeIndexQuery {
            snapshot: &snapshot,
            knowledge_kinds: &[KnowledgeKind::Rumor],
            limit: 8,
        })
        .await
        .expect("rumor index");
    let rumor_id = index[0].source_id.clone();
    for delivery in [
        KnowledgeDelivery::Writer,
        KnowledgeDelivery::Character {
            role_id: RoleId::try_new("witness").unwrap(),
        },
    ] {
        let filter = KnowledgeFilter {
            delivery,
            knowledge_kinds: vec![KnowledgeKind::Rumor],
            max_item_bytes: 4096,
        };
        let records = sqlite
            .find_by_source_ids(SourceKnowledgeQuery {
                snapshot: &snapshot,
                filter: &filter,
                source_ids: std::slice::from_ref(&rumor_id),
                limit: 1,
            })
            .await
            .expect("authorized rumor");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source_id, rumor_id);
    }
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn owner_memory_lookup_returns_only_exact_owner_records() {
    let (sqlite, snapshot, db) = seeded_store("owner_memory").await;
    let protagonist = RoleId::try_new("protagonist").unwrap();
    let witness = RoleId::try_new("witness").unwrap();
    let protagonist_records = sqlite
        .find_memories_by_owner(OwnerMemoryQuery {
            snapshot: &snapshot,
            owner: &protagonist,
            limit: 8,
            max_item_bytes: 4096,
        })
        .await
        .expect("protagonist memories");
    let witness_records = sqlite
        .find_memories_by_owner(OwnerMemoryQuery {
            snapshot: &snapshot,
            owner: &witness,
            limit: 8,
            max_item_bytes: 4096,
        })
        .await
        .expect("witness memories");
    assert_eq!(protagonist_records.len(), 1);
    assert_eq!(protagonist_records[0].memory_owner.as_ref(), Some(&protagonist));
    assert_eq!(protagonist_records[0].content.as_str(), "I arrived before dawn.");
    assert_eq!(witness_records.len(), 1);
    assert_eq!(witness_records[0].memory_owner.as_ref(), Some(&witness));
    assert_eq!(witness_records[0].content.as_str(), "I heard the bell at midnight.");
    assert_ne!(protagonist_records[0].source_id, witness_records[0].source_id);
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn knowledge_index_contains_only_fact_and_rumor_hints() {
    let (sqlite, snapshot, db) = seeded_store("index").await;
    let index = sqlite
        .list_index(KnowledgeIndexQuery {
            snapshot: &snapshot,
            knowledge_kinds: &[KnowledgeKind::Fact, KnowledgeKind::Rumor, KnowledgeKind::Memory],
            limit: 16,
        })
        .await
        .expect("index");
    assert_eq!(index.len(), 2);
    assert!(
        index
            .iter()
            .all(|record| matches!(&record.source_id, KnowledgeSourceId::Fact(_) | KnowledgeSourceId::Rumor(_)))
    );
    assert!(index.iter().all(|record| !record.retrieval_hint.as_str().is_empty()));
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn all_knowledge_reads_validate_snapshot_even_for_empty_queries() {
    let (sqlite, mut snapshot, db) = seeded_store("snapshot").await;
    snapshot.base_revision = aise::domain::ids::StoryRevision::new(999);
    let filter = KnowledgeFilter {
        delivery: KnowledgeDelivery::Writer,
        knowledge_kinds: vec![KnowledgeKind::Fact],
        max_item_bytes: 4096,
    };
    let source_result = sqlite
        .find_by_source_ids(SourceKnowledgeQuery {
            snapshot: &snapshot,
            filter: &filter,
            source_ids: &[],
            limit: 0,
        })
        .await;
    let owner = RoleId::try_new("protagonist").unwrap();
    let memory_result = sqlite
        .find_memories_by_owner(OwnerMemoryQuery {
            snapshot: &snapshot,
            owner: &owner,
            limit: 0,
            max_item_bytes: 4096,
        })
        .await;
    let index_result = sqlite
        .list_index(KnowledgeIndexQuery {
            snapshot: &snapshot,
            knowledge_kinds: &[],
            limit: 0,
        })
        .await;
    assert!(matches!(source_result, Err(aise::persistence::StoreError::RevisionConflict)));
    assert!(matches!(memory_result, Err(aise::persistence::StoreError::RevisionConflict)));
    assert!(matches!(index_result, Err(aise::persistence::StoreError::RevisionConflict)));
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn body_lookup_rejects_noncanonical_materialized_payload() {
    let (sqlite, snapshot, db) = seeded_store("materialize").await;
    let index = sqlite
        .list_index(KnowledgeIndexQuery {
            snapshot: &snapshot,
            knowledge_kinds: &[KnowledgeKind::Fact],
            limit: 1,
        })
        .await
        .expect("fact index");
    let fact_id = index[0].source_id.clone();
    sqlx::query(
        "UPDATE knowledge_entries SET payload_json = '{}' \
         WHERE story_id = ? AND knowledge_kind = 'fact' AND source_id = ?",
    )
    .bind(snapshot.story_id.as_str())
    .bind(fact_id.as_str())
    .execute(sqlite.pool_for_tests())
    .await
    .expect("corrupt payload");
    let filter = KnowledgeFilter {
        delivery: KnowledgeDelivery::Writer,
        knowledge_kinds: vec![KnowledgeKind::Fact],
        max_item_bytes: 4096,
    };
    let error = sqlite
        .find_by_source_ids(SourceKnowledgeQuery {
            snapshot: &snapshot,
            filter: &filter,
            source_ids: std::slice::from_ref(&fact_id),
            limit: 1,
        })
        .await
        .unwrap_err();
    assert!(matches!(error, aise::persistence::StoreError::Serialization { .. }));
    let _ = std::fs::remove_file(&db);
}
