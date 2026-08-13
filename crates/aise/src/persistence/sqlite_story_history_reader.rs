use crate::domain::ids::{StoryId, TurnId};
use crate::domain::story_sequence::StorySequence;
use crate::persistence::sqlite_error::SqliteStoreError;
use crate::persistence::sqlite_store::SqliteStore;
use crate::persistence::store::{StoreError, StoreSerializationErrorKind};
use crate::persistence::story_history_read_port::{
    StoryHistoryConfig, StoryHistoryPage, StoryHistoryQuery, StoryHistoryReadPort, StoryOpeningView, StoryTurnView,
};
use async_trait::async_trait;
use std::sync::Arc;

pub struct SqliteStoryHistoryReader {
    store: Arc<SqliteStore>,
    config: StoryHistoryConfig,
}

impl SqliteStoryHistoryReader {
    pub fn new(store: Arc<SqliteStore>, config: StoryHistoryConfig) -> Result<Self, &'static str> {
        config.validate()?;
        Ok(Self { store, config })
    }
}

#[async_trait]
impl StoryHistoryReadPort for SqliteStoryHistoryReader {
    async fn load_story_history(
        &self,
        story_id: &StoryId,
        query: StoryHistoryQuery,
    ) -> Result<StoryHistoryPage, StoreError> {
        load(&self.store, story_id, query, &self.config).await
    }
}

#[async_trait]
impl StoryHistoryReadPort for SqliteStore {
    async fn load_story_history(
        &self,
        story_id: &StoryId,
        query: StoryHistoryQuery,
    ) -> Result<StoryHistoryPage, StoreError> {
        load(self, story_id, query, &StoryHistoryConfig::default()).await
    }
}

#[async_trait]
impl StoryHistoryReadPort for Arc<SqliteStore> {
    async fn load_story_history(
        &self,
        story_id: &StoryId,
        query: StoryHistoryQuery,
    ) -> Result<StoryHistoryPage, StoreError> {
        StoryHistoryReadPort::load_story_history(&**self, story_id, query).await
    }
}

async fn load(
    store: &SqliteStore,
    story_id: &StoryId,
    query: StoryHistoryQuery,
    config: &StoryHistoryConfig,
) -> Result<StoryHistoryPage, StoreError> {
    if query.limit == 0 || query.limit > config.max_page_size {
        return Err(StoreError::LimitExceeded {
            limit: "story_history_page_size",
        });
    }
    let fetch_limit = query.limit.checked_add(1).ok_or(StoreError::LimitExceeded {
        limit: "story_history_page_size",
    })?;
    let opening_row: Option<(i64, String, i64, i64)> = sqlx::query_as(
        "SELECT sequence, story_text, created_at, length(CAST(story_text AS BLOB)) \
         FROM story_segments WHERE story_id = ?1 AND origin = 'opening'",
    )
    .bind(story_id.as_str())
    .fetch_optional(store.pool_for_tests())
    .await
    .map_err(SqliteStoreError::from)?;
    let opening = opening_row
        .map(|(sequence, story_text, created_at, story_bytes)| {
            if usize::try_from(story_bytes).map_err(|_| invalid_turn())? > config.max_story_text_bytes {
                return Err(StoreError::LimitExceeded {
                    limit: "max_story_text_bytes",
                });
            }
            Ok(StoryOpeningView {
                sequence: StorySequence::try_new(u64::try_from(sequence).map_err(|_| invalid_turn())?)
                    .map_err(|_| invalid_turn())?,
                story_text,
                created_at,
            })
        })
        .transpose()?;
    let after = query.after_sequence.map(|sequence| sequence.get()).unwrap_or(0);
    let rows: Vec<(String, i64, String, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT id, sequence, player_input, story_text, created_at, \
                length(CAST(player_input AS BLOB)), length(CAST(story_text AS BLOB)) \
         FROM story_turns WHERE world_id = ?1 AND sequence > ?2 \
         ORDER BY sequence ASC LIMIT ?3",
    )
    .bind(story_id.as_str())
    .bind(i64::try_from(after).map_err(|_| invalid_turn())?)
    .bind(i64::try_from(fetch_limit).map_err(|_| StoreError::LimitExceeded {
        limit: "story_history_page_size",
    })?)
    .fetch_all(store.pool_for_tests())
    .await
    .map_err(SqliteStoreError::from)?;
    let has_more = rows.len() > query.limit;
    let mut turns = Vec::with_capacity(query.limit.min(rows.len()));
    for (id, sequence, player_input, story_text, created_at, player_bytes, story_bytes) in
        rows.into_iter().take(query.limit)
    {
        if usize::try_from(player_bytes).map_err(|_| invalid_turn())? > config.max_player_input_bytes {
            return Err(StoreError::LimitExceeded {
                limit: "max_player_input_bytes",
            });
        }
        if usize::try_from(story_bytes).map_err(|_| invalid_turn())? > config.max_story_text_bytes {
            return Err(StoreError::LimitExceeded {
                limit: "max_story_text_bytes",
            });
        }
        turns.push(StoryTurnView {
            turn_id: TurnId::try_new(id).map_err(|_| invalid_turn())?,
            sequence: StorySequence::try_new(u64::try_from(sequence).map_err(|_| invalid_turn())?)
                .map_err(|_| invalid_turn())?,
            player_input,
            story_text,
            created_at,
        });
    }
    let next_after_sequence = if has_more {
        turns.last().map(|turn| turn.sequence)
    } else {
        None
    };
    Ok(StoryHistoryPage {
        opening,
        turns,
        next_after_sequence,
    })
}

fn invalid_turn() -> StoreError {
    StoreError::Serialization {
        kind: StoreSerializationErrorKind::InvalidTurnResult,
    }
}
