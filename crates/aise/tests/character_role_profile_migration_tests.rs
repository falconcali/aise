use sqlx::Row;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::borrow::Cow;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_character_role_profile_migration_{label}_{now}.db"))
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

async fn connect(db: &str) -> sqlx::SqlitePool {
    let options = SqliteConnectOptions::from_str(db)
        .unwrap()
        .create_if_missing(true)
        .foreign_keys(true);
    SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap()
}

async fn seed_story_pack_row(pool: &sqlx::SqlitePool) {
    let digest = "11".repeat(32);
    sqlx::query(
        "INSERT INTO story_packs (pack_id, pack_key, version, digest, pack_json, manifest_json, \
         characters_json, world_book_json, story_profile_json, role_definitions_json, \
         narrative_definition_json, topic_dictionary_json, resolved_characters_json) \
         VALUES ('pack-1', 'demo', '1.0.0', ?, '{}', ?, '{}', '{}', '{}', '{}', '{}', '{}', '{}')",
    )
    .bind(digest)
    .bind(Vec::<u8>::new())
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn migration_from_empty_schema_at_version_15_reaches_final_version() {
    let db = temp_db_path("empty_schema");
    let pool = connect(&db).await;
    migrator_through(15).run(&pool).await.unwrap();
    sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap();

    let orphans: Vec<sqlx::sqlite::SqliteRow> = sqlx::query("PRAGMA foreign_key_check").fetch_all(&pool).await.unwrap();
    assert!(orphans.is_empty(), "expected no foreign key violations after migration");

    let columns: Vec<String> = sqlx::query("PRAGMA table_info(story_packs)")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();
    assert!(!columns.iter().any(|name| name == "characters_json"));
    assert!(!columns.iter().any(|name| name == "resolved_characters_json"));

    let character_card_columns: Vec<String> = sqlx::query("PRAGMA table_info(character_cards)")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();
    for expected in [
        "character_id",
        "version",
        "digest",
        "card_json",
        "canonical_json",
        "created_at",
    ] {
        assert!(
            character_card_columns.iter().any(|name| name == expected),
            "expected character_cards to have column {expected}"
        );
    }
    pool.close().await;
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn migration_fails_on_populated_version_15_database_and_leaves_rows_unchanged() {
    let db = temp_db_path("populated_schema");
    let pool = connect(&db).await;
    migrator_through(15).run(&pool).await.unwrap();
    seed_story_pack_row(&pool).await;

    let error = sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap_err();
    assert!(error.to_string().contains("character_role_profile_legacy_data_present"));

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM story_packs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let digest: String = sqlx::query_scalar("SELECT digest FROM story_packs WHERE pack_id = 'pack-1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(digest, "11".repeat(32));
    pool.close().await;
    let _ = std::fs::remove_file(&db);
}
