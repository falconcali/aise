use crate::persistence::store::{StoreError, StoreSerializationErrorKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SqliteStoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<SqliteStoreError> for StoreError {
    fn from(error: SqliteStoreError) -> Self {
        match error {
            SqliteStoreError::Database(error) => map_database_error(error),
            SqliteStoreError::Migration(_) => StoreError::Unavailable,
            SqliteStoreError::Json(_) => StoreError::Serialization {
                kind: StoreSerializationErrorKind::InvalidStoryState,
            },
            SqliteStoreError::Io(_) => StoreError::Unavailable,
        }
    }
}

fn map_database_error(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(db) = &error {
        let code = db.code().map(|code| code.to_string()).unwrap_or_default();
        let message = db.message();
        if is_constraint_code(&code) {
            return StoreError::ConstraintViolation { constraint: code };
        }
        tracing::error!(error.message = message, error.code = code, "aise.store.database_error");
        return StoreError::Unavailable;
    }
    if is_unavailable(&error) {
        return StoreError::Unavailable;
    }
    tracing::error!(error = ?error, "aise.store.unexpected_error");
    StoreError::Unavailable
}

fn is_constraint_code(code: &str) -> bool {
    matches!(
        code,
        "19" | "787" | "2067" | "1555" | "1299" | "1811" | "262" | "257" | "275" | "531"
    )
}

fn is_unavailable(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::PoolTimedOut | sqlx::Error::WorkerCrashed | sqlx::Error::Io(_)
    )
}

pub trait SqliteResult<T> {
    fn to_store(self, kind: StoreSerializationErrorKind) -> Result<T, StoreError>;
}

impl<T> SqliteResult<T> for Result<T, sqlx::Error> {
    fn to_store(self, _kind: StoreSerializationErrorKind) -> Result<T, StoreError> {
        self.map_err(map_database_error)
    }
}

impl<T> SqliteResult<T> for Result<T, serde_json::Error> {
    fn to_store(self, kind: StoreSerializationErrorKind) -> Result<T, StoreError> {
        self.map_err(|_| StoreError::Serialization { kind })
    }
}
