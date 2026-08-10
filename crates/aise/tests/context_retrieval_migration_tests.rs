use aise::domain::knowledge::KnowledgeEntry;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::borrow::Cow;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_migration_{label}_{now}.db"))
        .to_string_lossy()
        .into_owned()
}

fn migrator_through(maximum: i64) -> Migrator {
    let all = sqlx::migrate!("./assets/persistence/mig");
    Migrator {
        migrations: Cow::Owned(all.iter().filter(|migration| migration.version <= maximum).cloned().collect()),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    }
}

async fn seed_version_eight(pool: &sqlx::SqlitePool) {
    let digest = "00".repeat(32);
    let pack = serde_json::json!({
        "story": {"premise":"demo"},
        "roles": {
            "hero": {
                "seed_memories": [{
                    "memory_key":"arrival",
                    "kind":"observed",
                    "content":"I arrived",
                    "topics":["arrival"],
                    "salience":60
                }]
            }
        },
        "narrative": {"nodes":{},"edges":[],"entry_nodes":[]}
    });
    let world = serde_json::json!({
        "topics":{"arrival":{"label":"Arrival","aliases":[]}},
        "facts":{"gate":{"proposition":null,"content":"The gate is open","entities":[{"kind":"location","key":"gate"}],"topics":["arrival"],"salience":80}},
        "rumors":{"bell":{"claim":null,"content":"The bell rings","entities":[{"kind":"event","key":"bell"}],"topics":["arrival"],"salience":40}}
    });
    sqlx::query(
        "INSERT INTO story_packs (pack_id, pack_key, version, digest, pack_json, manifest_json, characters_json, world_book_json) \
         VALUES ('pack-1','demo','1.0.0',?,?,?,?,?)",
    )
    .bind(digest)
    .bind(pack.to_string())
    .bind(Vec::<u8>::new())
    .bind("{}")
    .bind(world.to_string())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO stories (id, revision, player_character_id, created_at, current_scene, story_summary, active_constraints) \
         VALUES ('story-1',0,'char-1',1,'{}','{\"text\":\"\",\"summarized_through\":null}','[]')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO story_instances (story_id, pack_id, revision, bindings_json, characters_json, relationships_json, \
         facts_json, rumors_json, memories_json, narrative_state_json, created_at_ms) \
         VALUES ('story-1','pack-1',0,'{\"hero\":{\"character_id\":\"char-1\"}}','{}','[]','[]','[]','[]', \
         '{\"graph_revision\":0,\"node_states\":{},\"activation_turns\":{}}',1)",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn assert_final_migration(start_version: i64, label: &str) {
    let db = temp_db_path(label);
    let options = SqliteConnectOptions::from_str(&db)
        .unwrap()
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    migrator_through(8).run(&pool).await.unwrap();
    seed_version_eight(&pool).await;
    if start_version == 10 {
        migrator_through(10).run(&pool).await.unwrap();
    }
    sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap();
    let payloads: Vec<String> = sqlx::query_scalar(
        "SELECT payload_json FROM knowledge_entries WHERE story_id = 'story-1' ORDER BY knowledge_kind",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(payloads.len(), 3);
    for payload in payloads {
        serde_json::from_str::<KnowledgeEntry>(&payload).expect("typed knowledge payload");
    }
    let entity_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_entry_entities")
        .fetch_one(&pool)
        .await
        .unwrap();
    let topic_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_entry_topics")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(entity_count, 4);
    assert_eq!(topic_count, 3);
    let legacy_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('worlds','characters','memory')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(legacy_count, 0);
    pool.close().await;
    let _ = std::fs::remove_file(db);
}

#[tokio::test]
async fn migration_reconstructs_seed_knowledge_from_version_eight() {
    assert_final_migration(8, "v8").await;
}

#[tokio::test]
async fn migration_reconstructs_seed_knowledge_from_version_ten() {
    assert_final_migration(10, "v10").await;
}

#[tokio::test]
async fn migration_rejects_unrecoverable_committed_knowledge() {
    let db = temp_db_path("unrecoverable");
    let options = SqliteConnectOptions::from_str(&db)
        .unwrap()
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    migrator_through(8).run(&pool).await.unwrap();
    seed_version_eight(&pool).await;
    migrator_through(10).run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO knowledge_entries (story_id, source_id, knowledge_kind, memory_owner_character_id, content, \
         salience, source_json, source_revision) VALUES ('story-1','legacy:fact','fact',NULL,'legacy',50, \
         '{\"committed_turn\":{\"turn_id\":\"turn-1\",\"event_id\":null}}',1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let error = sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap_err();
    assert!(error.to_string().contains("context_retrieval_migration_unrecoverable"));
    let retained: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_entries WHERE source_id = 'legacy:fact'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(retained, 1);
    pool.close().await;
    let _ = std::fs::remove_file(db);
}

#[tokio::test]
async fn migration_rejects_ambiguous_summary_boundary() {
    let db = temp_db_path("summary");
    let options = SqliteConnectOptions::from_str(&db)
        .unwrap()
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    migrator_through(8).run(&pool).await.unwrap();
    seed_version_eight(&pool).await;
    sqlx::query("UPDATE stories SET story_summary = '{\"text\":\"ambiguous\",\"summarized_through\":null}'")
        .execute(&pool)
        .await
        .unwrap();
    let error = sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap_err();
    assert!(error.to_string().contains("context_retrieval_migration_unrecoverable"));
    let summary: String = sqlx::query_scalar("SELECT story_summary FROM stories WHERE id = 'story-1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(summary.contains("ambiguous"));
    pool.close().await;
    let _ = std::fs::remove_file(db);
}
