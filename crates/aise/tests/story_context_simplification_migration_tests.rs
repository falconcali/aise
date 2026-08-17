use sqlx::Row;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::borrow::Cow;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_story_context_simplification_migration_{label}_{now}.db"))
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
    let digest = "33".repeat(32);
    sqlx::query(
        "INSERT INTO story_packs (pack_id, pack_key, version, digest, pack_json, manifest_json, \
         world_book_json, story_profile_json, role_definitions_json, \
         narrative_definition_json, topic_dictionary_json) \
         VALUES ('pack-1', 'demo', '1.0.0', ?, '{}', ?, '{}', '{}', '{}', '{}', '{}')",
    )
    .bind(digest)
    .bind(Vec::<u8>::new())
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn story_context_simplification_applies_on_fresh_database() {
    let db = temp_db_path("fresh");
    let pool = connect(&db).await;
    sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap();
    pool.close().await;
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn story_context_simplification_applies_on_empty_upgraded_database() {
    let db = temp_db_path("empty_upgrade");
    let pool = connect(&db).await;
    migrator_through(18).run(&pool).await.unwrap();

    sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM story_packs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    pool.close().await;
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn story_context_simplification_migration_guard() {
    let db = temp_db_path("legacy_rows");
    let pool = connect(&db).await;
    migrator_through(18).run(&pool).await.unwrap();
    seed_story_pack_row(&pool).await;

    let error = sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap_err();
    assert!(error.to_string().contains("story_context_simplification_legacy_data_present"));

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM story_packs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let digest: String = sqlx::query_scalar("SELECT digest FROM story_packs WHERE pack_id = 'pack-1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(digest, "33".repeat(32));

    let columns: Vec<String> = sqlx::query("PRAGMA table_info(story_packs)")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();
    assert!(columns.iter().any(|name| name == "story_profile_json"));
    pool.close().await;
    let _ = std::fs::remove_file(&db);
}
