CREATE TEMP TABLE narrative_knowledge_context_migration_guard (
    value INTEGER CONSTRAINT narrative_knowledge_context_legacy_data_present CHECK (value = 0)
);

INSERT INTO narrative_knowledge_context_migration_guard
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_packs)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_instances)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM knowledge_entries);

DROP TABLE narrative_knowledge_context_migration_guard;

CREATE TABLE story_instances_new (
    story_id TEXT PRIMARY KEY REFERENCES stories(id) ON DELETE CASCADE,
    pack_id TEXT NOT NULL REFERENCES story_packs(pack_id),
    settings_json TEXT NOT NULL CHECK (json_valid(settings_json)),
    roles_json TEXT NOT NULL CHECK (json_valid(roles_json)),
    relationships_json TEXT NOT NULL CHECK (json_valid(relationships_json)),
    narrative_state_json TEXT NOT NULL CHECK (json_valid(narrative_state_json)),
    fact_values_json TEXT NOT NULL CHECK (json_valid(fact_values_json)),
    knowledge_id_high_water INTEGER NOT NULL DEFAULT 0 CHECK (knowledge_id_high_water >= 0),
    created_at_ms INTEGER NOT NULL
);

DROP TABLE story_instances;
ALTER TABLE story_instances_new RENAME TO story_instances;

CREATE TABLE knowledge_entries_new (
    story_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL,
    knowledge_kind TEXT NOT NULL CHECK (knowledge_kind IN ('fact', 'rumor', 'memory')),
    memory_owner_role_id TEXT,
    retrieval_hint TEXT,
    content TEXT NOT NULL,
    salience INTEGER NOT NULL CHECK (salience BETWEEN 0 AND 255),
    source_json TEXT NOT NULL CHECK (json_valid(source_json)),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    PRIMARY KEY (story_id, knowledge_kind, source_id),
    CHECK (
        (knowledge_kind = 'memory' AND memory_owner_role_id IS NOT NULL)
        OR (knowledge_kind != 'memory' AND memory_owner_role_id IS NULL)
    ),
    CHECK (
        (knowledge_kind = 'memory' AND retrieval_hint IS NULL)
        OR (knowledge_kind != 'memory' AND retrieval_hint IS NOT NULL AND TRIM(retrieval_hint) != '')
    ),
    CHECK (
        (knowledge_kind = 'fact'
            AND (source_id GLOB 'fact_[0-9][0-9][0-9][0-9]' OR source_id GLOB 'fact_[1-9][0-9][0-9][0-9][0-9]*'))
        OR (knowledge_kind = 'rumor'
            AND (source_id GLOB 'rumor_[0-9][0-9][0-9][0-9]' OR source_id GLOB 'rumor_[1-9][0-9][0-9][0-9][0-9]*'))
        OR (knowledge_kind = 'memory'
            AND (source_id GLOB 'memory_[0-9][0-9][0-9][0-9]' OR source_id GLOB 'memory_[1-9][0-9][0-9][0-9][0-9]*'))
    )
);

DROP TABLE knowledge_entries;
ALTER TABLE knowledge_entries_new RENAME TO knowledge_entries;

CREATE TABLE knowledge_entry_entities_new (
    story_id TEXT NOT NULL,
    knowledge_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    entity_kind TEXT NOT NULL CHECK (
        entity_kind IN ('world', 'role', 'location', 'scene', 'narrative_node', 'event')
    ),
    entity_key TEXT NOT NULL,
    PRIMARY KEY (story_id, knowledge_kind, source_id, entity_kind, entity_key),
    FOREIGN KEY (story_id, knowledge_kind, source_id)
        REFERENCES knowledge_entries(story_id, knowledge_kind, source_id)
        ON DELETE CASCADE
);

DROP TABLE knowledge_entry_entities;
ALTER TABLE knowledge_entry_entities_new RENAME TO knowledge_entry_entities;

CREATE TABLE knowledge_entry_topics_new (
    story_id TEXT NOT NULL,
    knowledge_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    topic_key TEXT NOT NULL,
    PRIMARY KEY (story_id, knowledge_kind, source_id, topic_key),
    FOREIGN KEY (story_id, knowledge_kind, source_id)
        REFERENCES knowledge_entries(story_id, knowledge_kind, source_id)
        ON DELETE CASCADE
);

DROP TABLE knowledge_entry_topics;
ALTER TABLE knowledge_entry_topics_new RENAME TO knowledge_entry_topics;

CREATE INDEX ix_knowledge_entry_entities_lookup
ON knowledge_entry_entities(story_id, entity_kind, entity_key, knowledge_kind, source_id);

CREATE INDEX ix_knowledge_entry_topics_lookup
ON knowledge_entry_topics(story_id, topic_key, knowledge_kind, source_id);

PRAGMA foreign_key_check;
