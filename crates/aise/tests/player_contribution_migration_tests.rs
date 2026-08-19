use sqlx::Row;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::borrow::Cow;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_player_contribution_migration_{label}_{now}.db"))
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

async fn connect(label: &str) -> (sqlx::SqlitePool, String) {
    let db = temp_db_path(label);
    let options = SqliteConnectOptions::from_str(&db)
        .unwrap()
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    (pool, db)
}

async fn story_turn_columns(pool: &sqlx::SqlitePool) -> Vec<String> {
    sqlx::query("PRAGMA table_info(story_turns)")
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect()
}

#[tokio::test]
async fn fresh_database_uses_player_contribution_column() {
    let (pool, db) = connect("fresh").await;
    sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap();
    let columns = story_turn_columns(&pool).await;
    assert!(columns.iter().any(|column| column == "player_contribution"));
    assert!(!columns.iter().any(|column| column == &["player", "input"].join("_")));
    pool.close().await;
    let _ = std::fs::remove_file(db);
}

#[tokio::test]
async fn version_twenty_one_upgrade_preserves_turn_data_and_constraints() {
    let (pool, db) = connect("upgrade").await;
    migrator_through(21).run(&pool).await.unwrap();
    sqlx::query("INSERT INTO stories (id, revision, player_role_id, created_at) VALUES ('story-1', 4, 'role-1', 100)")
        .execute(&pool)
        .await
        .unwrap();
    let legacy_column = ["player", "input"].join("_");
    let insert = format!(
        "INSERT INTO story_turns \
         (story_id, turn_number, {legacy_column}, story_text, status, created_at, idempotency_key, request_digest, \
          base_revision, committed_revision, result_json, sequence) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
    );
    let contribution = "你是谁\0我后退一步";
    sqlx::query(&insert)
        .bind("story-1")
        .bind(1_i64)
        .bind(contribution)
        .bind("你隔着门问：你是谁？")
        .bind("ok")
        .bind(101_i64)
        .bind("idem-1")
        .bind("digest-1")
        .bind(3_i64)
        .bind(4_i64)
        .bind(r#"{"turn_number":1,"story_text":"你隔着门问：你是谁？","committed_revision":4}"#)
        .bind(2_i64)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap();

    let columns = story_turn_columns(&pool).await;
    assert!(columns.iter().any(|column| column == "player_contribution"));
    assert!(!columns.iter().any(|column| column == &legacy_column));
    let row = sqlx::query(
        "SELECT story_id, turn_number, CAST(player_contribution AS BLOB) AS contribution, story_text, status, \
                created_at, idempotency_key, request_digest, base_revision, committed_revision, result_json, sequence \
         FROM story_turns",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("story_id"), "story-1");
    assert_eq!(row.get::<i64, _>("turn_number"), 1);
    assert_eq!(row.get::<Vec<u8>, _>("contribution"), contribution.as_bytes());
    assert_eq!(row.get::<String, _>("story_text"), "你隔着门问：你是谁？");
    assert_eq!(row.get::<String, _>("status"), "ok");
    assert_eq!(row.get::<i64, _>("created_at"), 101);
    assert_eq!(row.get::<String, _>("idempotency_key"), "idem-1");
    assert_eq!(row.get::<String, _>("request_digest"), "digest-1");
    assert_eq!(row.get::<i64, _>("base_revision"), 3);
    assert_eq!(row.get::<i64, _>("committed_revision"), 4);
    assert_eq!(row.get::<i64, _>("sequence"), 2);
    assert_eq!(
        row.get::<String, _>("result_json"),
        r#"{"turn_number":1,"story_text":"你隔着门问：你是谁？","committed_revision":4}"#
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM story_turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let duplicate = sqlx::query(
        "INSERT INTO story_turns \
         (story_id, turn_number, player_contribution, story_text, status, created_at, idempotency_key, request_digest, \
          base_revision, committed_revision, result_json, sequence) \
         VALUES ('story-1', 2, 'duplicate', 'duplicate', 'ok', 102, 'idem-1', 'digest-2', 4, 5, '{}', 3)",
    )
    .execute(&pool)
    .await;
    assert!(duplicate.is_err());
    pool.close().await;
    let _ = std::fs::remove_file(db);
}
