ALTER TABLE story_packs ADD COLUMN story_profile_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE story_packs ADD COLUMN role_definitions_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE story_packs ADD COLUMN narrative_definition_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE story_packs ADD COLUMN topic_dictionary_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE story_packs ADD COLUMN resolved_characters_json TEXT NOT NULL DEFAULT '{}';

CREATE TEMP TABLE context_retrieval_migration_guard (
    value INTEGER CONSTRAINT context_retrieval_migration_unrecoverable CHECK (value = 0)
);

INSERT INTO context_retrieval_migration_guard
SELECT 1
WHERE EXISTS (
    SELECT 1 FROM knowledge_entries
    WHERE source_revision != 0 OR json_type(source_json, '$.committed_turn') IS NOT NULL
);

INSERT INTO context_retrieval_migration_guard
SELECT 1
WHERE EXISTS (
    SELECT 1 FROM stories
    WHERE NOT json_valid(story_summary)
       OR (
            TRIM(COALESCE(json_extract(story_summary, '$.text'), '')) != ''
            AND json_extract(story_summary, '$.summarized_through') IS NULL
       )
);

UPDATE story_packs
SET story_profile_json = json_extract(pack_json, '$.story'),
    role_definitions_json = json_extract(pack_json, '$.roles'),
    narrative_definition_json = json_extract(pack_json, '$.narrative'),
    topic_dictionary_json = COALESCE(json_extract(world_book_json, '$.topics'), '{}'),
    resolved_characters_json = characters_json;

CREATE TABLE knowledge_entries_new (
    story_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL,
    knowledge_kind TEXT NOT NULL CHECK (knowledge_kind IN ('fact', 'rumor', 'memory')),
    memory_owner_character_id TEXT,
    content TEXT NOT NULL,
    salience INTEGER NOT NULL CHECK (salience BETWEEN 0 AND 255),
    source_json TEXT NOT NULL CHECK (json_valid(source_json)),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    source_revision INTEGER NOT NULL CHECK (source_revision >= 0),
    PRIMARY KEY (story_id, knowledge_kind, source_id),
    CHECK (
        (knowledge_kind = 'memory' AND memory_owner_character_id IS NOT NULL)
        OR (knowledge_kind != 'memory' AND memory_owner_character_id IS NULL)
    )
);

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

INSERT INTO knowledge_entries_new (
    story_id, source_id, knowledge_kind, memory_owner_character_id, content,
    salience, source_json, payload_json, source_revision
)
SELECT i.story_id,
       i.story_id || ':seed:fact:' || fact.key,
       'fact',
       NULL,
       json_extract(fact.value, '$.content'),
       json_extract(fact.value, '$.salience'),
       json_object('seed', json_object('pack_id', p.pack_id, 'pack_digest', p.digest)),
       json_object(
           'kind', 'fact',
           'value', json_object(
               'id', i.story_id || ':seed:fact:' || fact.key,
               'key', fact.key,
               'text', json_extract(fact.value, '$.content'),
               'proposition', json(json_extract(fact.value, '$.proposition')),
               'entities', json(json_extract(fact.value, '$.entities')),
               'topics', json(json_extract(fact.value, '$.topics')),
               'salience', json_extract(fact.value, '$.salience'),
               'source', json(json_object('seed', json_object('pack_id', p.pack_id, 'pack_digest', p.digest))),
               'story_revision', 0
           )
       ),
       0
FROM story_instances i
JOIN story_packs p ON p.pack_id = i.pack_id
JOIN json_each(p.world_book_json, '$.facts') fact;

INSERT INTO knowledge_entries_new (
    story_id, source_id, knowledge_kind, memory_owner_character_id, content,
    salience, source_json, payload_json, source_revision
)
SELECT i.story_id,
       i.story_id || ':seed:rumor:' || rumor.key,
       'rumor',
       NULL,
       json_extract(rumor.value, '$.content'),
       json_extract(rumor.value, '$.salience'),
       json_object('seed', json_object('pack_id', p.pack_id, 'pack_digest', p.digest)),
       json_object(
           'kind', 'rumor',
           'value', json_object(
               'id', i.story_id || ':seed:rumor:' || rumor.key,
               'key', rumor.key,
               'content', json_extract(rumor.value, '$.content'),
               'claim', json(json_extract(rumor.value, '$.claim')),
               'entities', json(json_extract(rumor.value, '$.entities')),
               'topics', json(json_extract(rumor.value, '$.topics')),
               'salience', json_extract(rumor.value, '$.salience'),
               'source_role_key', NULL,
               'source_character_id', NULL,
               'truth_value', 'unverified',
               'source', json(json_object('seed', json_object('pack_id', p.pack_id, 'pack_digest', p.digest))),
               'story_revision', 0
           )
       ),
       0
FROM story_instances i
JOIN story_packs p ON p.pack_id = i.pack_id
JOIN json_each(p.world_book_json, '$.rumors') rumor;

INSERT INTO knowledge_entries_new (
    story_id, source_id, knowledge_kind, memory_owner_character_id, content,
    salience, source_json, payload_json, source_revision
)
SELECT i.story_id,
       i.story_id || ':seed:memory:' || role.key || ':' || json_extract(memory.value, '$.memory_key'),
       'memory',
       json_extract(binding.value, '$.character_id'),
       json_extract(memory.value, '$.content'),
       json_extract(memory.value, '$.salience'),
       json_object('seed', json_object('pack_id', p.pack_id, 'pack_digest', p.digest)),
       json_object(
           'kind', 'memory',
           'value', json_object(
               'id', i.story_id || ':seed:memory:' || role.key || ':' || json_extract(memory.value, '$.memory_key'),
               'owner', json_extract(binding.value, '$.character_id'),
               'kind', json_extract(memory.value, '$.kind'),
               'content', json_extract(memory.value, '$.content'),
               'entities', json_array(
                   json_object('kind', 'role', 'key', role.key),
                   json_object('kind', 'character', 'key', json_extract(binding.value, '$.character_id'))
               ),
               'topics', json(json_extract(memory.value, '$.topics')),
               'salience', json_extract(memory.value, '$.salience'),
               'source', json(json_object('seed', json_object('pack_id', p.pack_id, 'pack_digest', p.digest))),
               'story_revision', 0,
               'created_at_ms', i.created_at_ms
           )
       ),
       0
FROM story_instances i
JOIN story_packs p ON p.pack_id = i.pack_id
JOIN json_each(p.pack_json, '$.roles') role
JOIN json_each(role.value, '$.seed_memories') memory
JOIN json_each(i.bindings_json) binding ON binding.key = role.key;

INSERT INTO knowledge_entry_entities_new
SELECT i.story_id, 'fact', i.story_id || ':seed:fact:' || fact.key,
       json_extract(entity.value, '$.kind'), json_extract(entity.value, '$.key')
FROM story_instances i
JOIN story_packs p ON p.pack_id = i.pack_id
JOIN json_each(p.world_book_json, '$.facts') fact
JOIN json_each(fact.value, '$.entities') entity;

INSERT INTO knowledge_entry_entities_new
SELECT i.story_id, 'rumor', i.story_id || ':seed:rumor:' || rumor.key,
       json_extract(entity.value, '$.kind'), json_extract(entity.value, '$.key')
FROM story_instances i
JOIN story_packs p ON p.pack_id = i.pack_id
JOIN json_each(p.world_book_json, '$.rumors') rumor
JOIN json_each(rumor.value, '$.entities') entity;

INSERT INTO knowledge_entry_entities_new
SELECT i.story_id, 'memory',
       i.story_id || ':seed:memory:' || role.key || ':' || json_extract(memory.value, '$.memory_key'),
       'role', role.key
FROM story_instances i
JOIN story_packs p ON p.pack_id = i.pack_id
JOIN json_each(p.pack_json, '$.roles') role
JOIN json_each(role.value, '$.seed_memories') memory
UNION ALL
SELECT i.story_id, 'memory',
       i.story_id || ':seed:memory:' || role.key || ':' || json_extract(memory.value, '$.memory_key'),
       'character', json_extract(binding.value, '$.character_id')
FROM story_instances i
JOIN story_packs p ON p.pack_id = i.pack_id
JOIN json_each(p.pack_json, '$.roles') role
JOIN json_each(role.value, '$.seed_memories') memory
JOIN json_each(i.bindings_json) binding ON binding.key = role.key;

INSERT INTO knowledge_entry_topics_new
SELECT i.story_id, 'fact', i.story_id || ':seed:fact:' || fact.key, topic.value
FROM story_instances i
JOIN story_packs p ON p.pack_id = i.pack_id
JOIN json_each(p.world_book_json, '$.facts') fact
JOIN json_each(fact.value, '$.topics') topic;

INSERT INTO knowledge_entry_topics_new
SELECT i.story_id, 'rumor', i.story_id || ':seed:rumor:' || rumor.key, topic.value
FROM story_instances i
JOIN story_packs p ON p.pack_id = i.pack_id
JOIN json_each(p.world_book_json, '$.rumors') rumor
JOIN json_each(rumor.value, '$.topics') topic;

INSERT INTO knowledge_entry_topics_new
SELECT i.story_id, 'memory',
       i.story_id || ':seed:memory:' || role.key || ':' || json_extract(memory.value, '$.memory_key'),
       topic.value
FROM story_instances i
JOIN story_packs p ON p.pack_id = i.pack_id
JOIN json_each(p.pack_json, '$.roles') role
JOIN json_each(role.value, '$.seed_memories') memory
JOIN json_each(memory.value, '$.topics') topic;

INSERT INTO context_retrieval_migration_guard
SELECT 1
WHERE (SELECT COUNT(*) FROM knowledge_entries_new WHERE knowledge_kind = 'fact') != (
    SELECT COUNT(*) FROM story_instances i
    JOIN story_packs p ON p.pack_id = i.pack_id
    JOIN json_each(p.world_book_json, '$.facts') fact
);

INSERT INTO context_retrieval_migration_guard
SELECT 1
WHERE (SELECT COUNT(*) FROM knowledge_entries_new WHERE knowledge_kind = 'rumor') != (
    SELECT COUNT(*) FROM story_instances i
    JOIN story_packs p ON p.pack_id = i.pack_id
    JOIN json_each(p.world_book_json, '$.rumors') rumor
);

INSERT INTO context_retrieval_migration_guard
SELECT 1
WHERE (SELECT COUNT(*) FROM knowledge_entries_new WHERE knowledge_kind = 'memory') != (
    SELECT COUNT(*) FROM story_instances i
    JOIN story_packs p ON p.pack_id = i.pack_id
    JOIN json_each(p.pack_json, '$.roles') role
    JOIN json_each(role.value, '$.seed_memories') memory
);

INSERT INTO context_retrieval_migration_guard
SELECT 1
WHERE (SELECT COUNT(*) FROM knowledge_entry_entities_new) != (
    SELECT COUNT(*) FROM story_instances i
    JOIN story_packs p ON p.pack_id = i.pack_id
    JOIN json_each(p.world_book_json, '$.facts') fact
    JOIN json_each(fact.value, '$.entities') entity
) + (
    SELECT COUNT(*) FROM story_instances i
    JOIN story_packs p ON p.pack_id = i.pack_id
    JOIN json_each(p.world_book_json, '$.rumors') rumor
    JOIN json_each(rumor.value, '$.entities') entity
) + 2 * (
    SELECT COUNT(*) FROM story_instances i
    JOIN story_packs p ON p.pack_id = i.pack_id
    JOIN json_each(p.pack_json, '$.roles') role
    JOIN json_each(role.value, '$.seed_memories') memory
);

INSERT INTO context_retrieval_migration_guard
SELECT 1
WHERE (SELECT COUNT(*) FROM knowledge_entry_topics_new) != (
    SELECT COUNT(*) FROM story_instances i
    JOIN story_packs p ON p.pack_id = i.pack_id
    JOIN json_each(p.world_book_json, '$.facts') fact
    JOIN json_each(fact.value, '$.topics') topic
) + (
    SELECT COUNT(*) FROM story_instances i
    JOIN story_packs p ON p.pack_id = i.pack_id
    JOIN json_each(p.world_book_json, '$.rumors') rumor
    JOIN json_each(rumor.value, '$.topics') topic
) + (
    SELECT COUNT(*) FROM story_instances i
    JOIN story_packs p ON p.pack_id = i.pack_id
    JOIN json_each(p.pack_json, '$.roles') role
    JOIN json_each(role.value, '$.seed_memories') memory
    JOIN json_each(memory.value, '$.topics') topic
);

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

CREATE TABLE story_instances_new (
    story_id TEXT PRIMARY KEY REFERENCES stories(id) ON DELETE CASCADE,
    pack_id TEXT NOT NULL REFERENCES story_packs(pack_id),
    settings_json TEXT NOT NULL CHECK (json_valid(settings_json)),
    bindings_json TEXT NOT NULL CHECK (json_valid(bindings_json)),
    characters_json TEXT NOT NULL CHECK (json_valid(characters_json)),
    relationships_json TEXT NOT NULL CHECK (json_valid(relationships_json)),
    current_perceptions_json TEXT NOT NULL CHECK (json_valid(current_perceptions_json)),
    narrative_state_json TEXT NOT NULL CHECK (json_valid(narrative_state_json)),
    condition_state_json TEXT NOT NULL CHECK (json_valid(condition_state_json)),
    created_at_ms INTEGER NOT NULL
);

INSERT INTO story_instances_new (
    story_id, pack_id, settings_json, bindings_json, characters_json,
    relationships_json, current_perceptions_json, narrative_state_json,
    condition_state_json, created_at_ms
)
SELECT story_id, pack_id, '{}', bindings_json, characters_json,
       relationships_json, '[]', narrative_state_json,
       '{"occurred_event_keys":[],"player_action_event_keys":[],"fact_values":{}}', created_at_ms
FROM story_instances;

DROP TABLE story_instances;
ALTER TABLE story_instances_new RENAME TO story_instances;

CREATE TABLE story_turns_new (
    id TEXT PRIMARY KEY,
    world_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    player_input TEXT NOT NULL,
    story_text TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    base_revision INTEGER NOT NULL,
    committed_revision INTEGER NOT NULL,
    result_json TEXT NOT NULL CHECK (json_valid(result_json)),
    sequence INTEGER NOT NULL,
    UNIQUE(world_id, idempotency_key),
    UNIQUE(world_id, sequence)
);

INSERT INTO story_turns_new (
    id, world_id, player_input, story_text, status, created_at,
    idempotency_key, request_digest, base_revision, committed_revision,
    result_json, sequence
)
SELECT id, world_id, player_input, story_text, status, created_at,
       idempotency_key, request_digest, base_revision, committed_revision,
       result_json, sequence
FROM story_turns;

CREATE TABLE story_events_new (
    id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL REFERENCES story_turns_new(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    kind TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    UNIQUE (turn_id, seq)
);

INSERT INTO story_events_new SELECT id, turn_id, seq, kind, payload FROM story_events;
DROP TABLE story_events;
DROP TABLE story_turns;
ALTER TABLE story_turns_new RENAME TO story_turns;
ALTER TABLE story_events_new RENAME TO story_events;

DROP TABLE memory;
DROP TABLE characters;
DROP TABLE worlds;

PRAGMA foreign_key_check;

DROP TABLE context_retrieval_migration_guard;
