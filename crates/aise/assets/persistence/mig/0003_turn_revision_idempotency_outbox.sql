CREATE TABLE IF NOT EXISTS stories (
    id                  TEXT PRIMARY KEY,
    revision            INTEGER NOT NULL,
    player_character_id TEXT,
    created_at          INTEGER NOT NULL
);

ALTER TABLE story_turns ADD COLUMN idempotency_key TEXT;
ALTER TABLE story_turns ADD COLUMN request_digest TEXT;
ALTER TABLE story_turns ADD COLUMN base_revision INTEGER;
ALTER TABLE story_turns ADD COLUMN committed_revision INTEGER;
ALTER TABLE story_turns ADD COLUMN result_json TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_story_turns_story_idempotency
    ON story_turns(world_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE TABLE IF NOT EXISTS outbox (
    id              TEXT PRIMARY KEY,
    story_id        TEXT NOT NULL,
    turn_id         TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    payload         TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    attempt_count   INTEGER NOT NULL,
    published_at    INTEGER,
    last_error      TEXT
);

CREATE INDEX IF NOT EXISTS idx_outbox_unpublished
    ON outbox(created_at)
    WHERE published_at IS NULL;
