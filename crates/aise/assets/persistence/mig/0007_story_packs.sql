CREATE TABLE IF NOT EXISTS story_packs (
    pack_id          TEXT PRIMARY KEY,
    pack_key         TEXT NOT NULL,
    version          TEXT NOT NULL,
    digest           TEXT NOT NULL UNIQUE,
    pack_json        TEXT NOT NULL,
    manifest_json    BLOB NOT NULL,
    characters_json  TEXT NOT NULL,
    world_book_json  TEXT NOT NULL,
    created_at       INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_story_packs_key_version ON story_packs (pack_key, version);
