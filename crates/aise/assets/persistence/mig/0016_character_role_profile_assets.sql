CREATE TEMP TABLE character_role_profile_migration_guard (
    value INTEGER CONSTRAINT character_role_profile_legacy_data_present CHECK (value = 0)
);

INSERT INTO character_role_profile_migration_guard
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_packs)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_instances);

CREATE TABLE story_packs_new (
    pack_id                   TEXT PRIMARY KEY,
    pack_key                  TEXT NOT NULL,
    version                   TEXT NOT NULL,
    digest                    TEXT NOT NULL UNIQUE,
    pack_json                 TEXT NOT NULL CHECK (json_valid(pack_json)),
    manifest_json             BLOB NOT NULL,
    world_book_json           TEXT NOT NULL CHECK (json_valid(world_book_json)),
    story_profile_json        TEXT NOT NULL CHECK (json_valid(story_profile_json)),
    role_definitions_json     TEXT NOT NULL CHECK (json_valid(role_definitions_json)),
    narrative_definition_json TEXT NOT NULL CHECK (json_valid(narrative_definition_json)),
    topic_dictionary_json     TEXT NOT NULL CHECK (json_valid(topic_dictionary_json)),
    created_at                INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (pack_key, version)
);

INSERT INTO story_packs_new (
    pack_id, pack_key, version, digest, pack_json, manifest_json,
    world_book_json, story_profile_json, role_definitions_json,
    narrative_definition_json, topic_dictionary_json, created_at
)
SELECT pack_id, pack_key, version, digest, pack_json, manifest_json,
       world_book_json, story_profile_json, role_definitions_json,
       narrative_definition_json, topic_dictionary_json, created_at
FROM story_packs;

DROP TABLE story_packs;
ALTER TABLE story_packs_new RENAME TO story_packs;

CREATE INDEX idx_story_packs_key_version ON story_packs (pack_key, version);

CREATE TABLE character_cards (
    character_id  TEXT NOT NULL,
    version       TEXT NOT NULL,
    digest        TEXT NOT NULL UNIQUE,
    card_json     TEXT NOT NULL CHECK (json_valid(card_json)),
    canonical_json BLOB NOT NULL,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (character_id, version)
);

PRAGMA foreign_key_check;

DROP TABLE character_role_profile_migration_guard;
