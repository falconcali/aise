CREATE TEMP TABLE world_info_activation_migration_guard (
    value INTEGER CONSTRAINT world_info_activation_legacy_data_present CHECK (value = 0)
);

INSERT INTO world_info_activation_migration_guard
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_packs)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_instances)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM knowledge_entries);

DROP TABLE world_info_activation_migration_guard;

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
    created_at                INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (pack_key, version)
);

DROP TABLE story_packs;
ALTER TABLE story_packs_new RENAME TO story_packs;
CREATE INDEX idx_story_packs_key_version ON story_packs (pack_key, version);

ALTER TABLE story_instances ADD COLUMN activation_overlay_version INTEGER NOT NULL DEFAULT 0
    CHECK (activation_overlay_version >= 0);

CREATE TABLE knowledge_entries_new (
    story_id                   TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    source_id                  TEXT NOT NULL,
    knowledge_kind             TEXT NOT NULL CHECK (knowledge_kind IN ('fact', 'rumor', 'memory')),
    memory_owner_role_id       TEXT,
    retrieval_hint             TEXT,
    content                    TEXT NOT NULL,
    salience                   INTEGER NOT NULL CHECK (salience BETWEEN 0 AND 255),
    source_json                TEXT NOT NULL CHECK (json_valid(source_json)),
    payload_json               TEXT NOT NULL CHECK (json_valid(payload_json)),
    activation_rule_json       TEXT,
    activation_rule_version    TEXT,
    PRIMARY KEY (story_id, knowledge_kind, source_id),
    CHECK (
        (knowledge_kind = 'memory' AND memory_owner_role_id IS NOT NULL
            AND activation_rule_json IS NULL AND activation_rule_version IS NULL)
        OR (knowledge_kind != 'memory' AND memory_owner_role_id IS NULL
            AND activation_rule_json IS NOT NULL AND activation_rule_version IS NOT NULL)
    ),
    CHECK (
        (knowledge_kind = 'memory' AND retrieval_hint IS NULL)
        OR (knowledge_kind != 'memory' AND retrieval_hint IS NOT NULL AND TRIM(retrieval_hint) != '')
    )
);

DROP TABLE knowledge_entries;
ALTER TABLE knowledge_entries_new RENAME TO knowledge_entries;
DROP TABLE knowledge_entry_entities;
DROP TABLE knowledge_entry_topics;

CREATE TABLE knowledge_activation_timed_state (
    story_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL,
    rule_version TEXT NOT NULL,
    sticky_through_turn INTEGER,
    cooldown_through_turn INTEGER,
    PRIMARY KEY (story_id, source_id),
    CHECK (sticky_through_turn IS NULL OR sticky_through_turn > 0),
    CHECK (cooldown_through_turn IS NULL OR cooldown_through_turn > 0)
);

PRAGMA foreign_key_check;
