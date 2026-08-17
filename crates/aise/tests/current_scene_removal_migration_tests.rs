use sqlx::Row;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::borrow::Cow;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(label: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("aise_current_scene_removal_migration_{label}_{now}.db"))
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

async fn stories_columns(pool: &sqlx::SqlitePool) -> Vec<String> {
    sqlx::query("PRAGMA table_info(stories)")
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect()
}

#[tokio::test]
async fn persistence_migration_drops_current_scene() {
    let db = temp_db_path("fresh");
    let pool = connect(&db).await;
    sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap();

    let columns = stories_columns(&pool).await;
    assert!(
        !columns.iter().any(|name| name == "current_scene"),
        "expected fresh schema to have no stories.current_scene column"
    );
    pool.close().await;
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn persistence_migration_drops_current_scene_on_upgrade() {
    let db = temp_db_path("upgrade");
    let pool = connect(&db).await;
    migrator_through(17).run(&pool).await.unwrap();

    let before = stories_columns(&pool).await;
    assert!(
        before.iter().any(|name| name == "current_scene"),
        "expected pre-migration schema to still carry stories.current_scene"
    );

    sqlx::migrate!("./assets/persistence/mig").run(&pool).await.unwrap();

    let after = stories_columns(&pool).await;
    assert!(
        !after.iter().any(|name| name == "current_scene"),
        "expected upgraded schema to have no stories.current_scene column"
    );
    pool.close().await;
    let _ = std::fs::remove_file(&db);
}
