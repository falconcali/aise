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
        if code.contains("2067") || code.contains("1555") || code.contains("19") {
            return StoreError::ConstraintViolation { constraint: code };
        }
    }
    if is_unavailable(&error) {
        return StoreError::Unavailable;
    }
    StoreError::Unavailable
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
