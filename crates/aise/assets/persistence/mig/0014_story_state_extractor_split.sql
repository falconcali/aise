CREATE TABLE story_instances_new (
    story_id TEXT PRIMARY KEY REFERENCES stories(id) ON DELETE CASCADE,
    pack_id TEXT NOT NULL REFERENCES story_packs(pack_id),
    settings_json TEXT NOT NULL CHECK (json_valid(settings_json)),
    bindings_json TEXT NOT NULL CHECK (json_valid(bindings_json)),
    characters_json TEXT NOT NULL CHECK (json_valid(characters_json)),
    relationships_json TEXT NOT NULL CHECK (json_valid(relationships_json)),
    narrative_state_json TEXT NOT NULL CHECK (json_valid(narrative_state_json)),
    condition_state_json TEXT NOT NULL CHECK (json_valid(condition_state_json)),
    created_at_ms INTEGER NOT NULL
);

INSERT INTO story_instances_new (
    story_id, pack_id, settings_json, bindings_json, characters_json,
    relationships_json, narrative_state_json, condition_state_json, created_at_ms
)
SELECT story_id, pack_id, settings_json, bindings_json, characters_json,
       relationships_json, narrative_state_json, condition_state_json, created_at_ms
FROM story_instances;

DROP TABLE story_instances;
ALTER TABLE story_instances_new RENAME TO story_instances;

CREATE TABLE knowledge_entries_new (
    story_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL,
    knowledge_kind TEXT NOT NULL CHECK (knowledge_kind IN ('fact', 'rumor', 'memory')),
    memory_owner_character_id TEXT,
    content TEXT NOT NULL,
    salience INTEGER NOT NULL CHECK (salience BETWEEN 0 AND 255),
    source_json TEXT NOT NULL CHECK (json_valid(source_json)),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    PRIMARY KEY (story_id, knowledge_kind, source_id),
    CHECK (
        (knowledge_kind = 'memory' AND memory_owner_character_id IS NOT NULL)
        OR (knowledge_kind != 'memory' AND memory_owner_character_id IS NULL)
    )
);

INSERT INTO knowledge_entries_new (
    story_id, source_id, knowledge_kind, memory_owner_character_id, content,
    salience, source_json, payload_json
)
SELECT story_id, source_id, knowledge_kind, memory_owner_character_id, content,
       salience, source_json, json_remove(payload_json, '$.value.story_revision')
FROM knowledge_entries;

CREATE TABLE knowledge_entry_entities_new (
    story_id TEXT NOT NULL,
    knowledge_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    entity_kind TEXT NOT NULL,
    entity_key TEXT NOT NULL,
    PRIMARY KEY (story_id, knowledge_kind, source_id, entity_kind, entity_key),
    FOREIGN KEY (story_id, knowledge_kind, source_id)
        REFERENCES knowledge_entries_new(story_id, knowledge_kind, source_id)
        ON DELETE CASCADE
);

INSERT INTO knowledge_entry_entities_new
SELECT story_id, knowledge_kind, source_id, entity_kind, entity_key
FROM knowledge_entry_entities;

CREATE TABLE knowledge_entry_topics_new (
    story_id TEXT NOT NULL,
    knowledge_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    topic_key TEXT NOT NULL,
    PRIMARY KEY (story_id, knowledge_kind, source_id, topic_key),
    FOREIGN KEY (story_id, knowledge_kind, source_id)
        REFERENCES knowledge_entries_new(story_id, knowledge_kind, source_id)
        ON DELETE CASCADE
);

INSERT INTO knowledge_entry_topics_new
SELECT story_id, knowledge_kind, source_id, topic_key
FROM knowledge_entry_topics;

DROP TABLE knowledge_entry_entities;
DROP TABLE knowledge_entry_topics;
DROP TABLE knowledge_entries;
ALTER TABLE knowledge_entries_new RENAME TO knowledge_entries;
ALTER TABLE knowledge_entry_entities_new RENAME TO knowledge_entry_entities;
ALTER TABLE knowledge_entry_topics_new RENAME TO knowledge_entry_topics;

CREATE INDEX ix_knowledge_entry_entities_lookup
ON knowledge_entry_entities(story_id, entity_kind, entity_key, knowledge_kind, source_id);

CREATE INDEX ix_knowledge_entry_topics_lookup
ON knowledge_entry_topics(story_id, topic_key, knowledge_kind, source_id);

PRAGMA foreign_key_check;
