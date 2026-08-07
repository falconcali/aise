CREATE TABLE IF NOT EXISTS story_instances (
    story_id              TEXT PRIMARY KEY REFERENCES stories(id),
    pack_id               TEXT NOT NULL REFERENCES story_packs(pack_id),
    revision              INTEGER NOT NULL,
    bindings_json         TEXT NOT NULL,
    characters_json       TEXT NOT NULL,
    relationships_json    TEXT NOT NULL,
    facts_json            TEXT NOT NULL,
    rumors_json           TEXT NOT NULL,
    memories_json         TEXT NOT NULL,
    narrative_state_json  TEXT NOT NULL,
    created_at_ms         INTEGER NOT NULL
);
