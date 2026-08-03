-- Initial schema for aise.
-- Created_at values are Unix milliseconds.

CREATE TABLE IF NOT EXISTS worlds (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    state       TEXT NOT NULL, -- serialized WorldState
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS characters (
    id          TEXT PRIMARY KEY,
    world_id    TEXT NOT NULL REFERENCES worlds(id),
    state       TEXT NOT NULL, -- serialized CharacterState
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS story_turns (
    id              TEXT PRIMARY KEY,
    world_id        TEXT NOT NULL REFERENCES worlds(id),
    player_input    TEXT NOT NULL,
    story_text      TEXT NOT NULL,
    summary_delta   TEXT,
    status          TEXT NOT NULL, -- 'committed' | 'failed'
    created_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS story_events (
    id          TEXT PRIMARY KEY,
    turn_id     TEXT NOT NULL REFERENCES story_turns(id),
    seq         INTEGER NOT NULL,
    kind        TEXT NOT NULL,
    payload     TEXT NOT NULL,
    UNIQUE (turn_id, seq)
);

CREATE TABLE IF NOT EXISTS memory (
    id           TEXT PRIMARY KEY,
    character_id TEXT NOT NULL REFERENCES characters(id),
    kind         TEXT NOT NULL,
    content      TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);
