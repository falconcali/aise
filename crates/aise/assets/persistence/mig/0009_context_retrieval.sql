ALTER TABLE story_turns ADD COLUMN sequence INTEGER;

WITH ordered AS (
    SELECT
        rowid AS rid,
        world_id,
        ROW_NUMBER() OVER (
            PARTITION BY world_id
            ORDER BY created_at ASC, rowid ASC
        ) AS seq
    FROM story_turns
)
UPDATE story_turns
SET sequence = (
    SELECT ordered.seq
    FROM ordered
    WHERE ordered.rid = story_turns.rowid
)
WHERE sequence IS NULL;

UPDATE stories
SET story_summary = CASE
    WHEN TRIM(COALESCE(story_summary, '')) = '' THEN '{"text":"","summarized_through":null}'
    WHEN json_valid(story_summary)
         AND json_extract(story_summary, '$.summarized_through') IS NULL
         AND TRIM(COALESCE(json_extract(story_summary, '$.text'), '')) = ''
        THEN json_set(story_summary, '$.summarized_through', NULL)
    WHEN json_valid(story_summary)
         AND json_extract(story_summary, '$.summarized_through') IS NOT NULL
        THEN story_summary
    WHEN json_valid(story_summary)
         AND TRIM(COALESCE(json_extract(story_summary, '$.text'), '')) = ''
        THEN json_set(story_summary, '$.summarized_through', NULL)
    ELSE story_summary
END;

CREATE UNIQUE INDEX IF NOT EXISTS ux_story_turns_world_sequence
ON story_turns(world_id, sequence);

CREATE TABLE IF NOT EXISTS knowledge_entries (
    story_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    knowledge_kind TEXT NOT NULL CHECK (knowledge_kind IN ('fact', 'rumor', 'memory')),
    memory_owner_character_id TEXT NULL,
    content TEXT NOT NULL,
    salience INTEGER NOT NULL CHECK (salience >= 0 AND salience <= 255),
    source_json TEXT NOT NULL,
    source_revision INTEGER NOT NULL,
    PRIMARY KEY (story_id, knowledge_kind, source_id),
    CHECK (
        (knowledge_kind = 'memory' AND memory_owner_character_id IS NOT NULL)
        OR (knowledge_kind != 'memory' AND memory_owner_character_id IS NULL)
    )
);

CREATE TABLE IF NOT EXISTS knowledge_entry_entities (
    story_id TEXT NOT NULL,
    knowledge_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    entity_kind TEXT NOT NULL,
    entity_key TEXT NOT NULL,
    PRIMARY KEY (story_id, knowledge_kind, source_id, entity_kind, entity_key),
    FOREIGN KEY (story_id, knowledge_kind, source_id)
        REFERENCES knowledge_entries(story_id, knowledge_kind, source_id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS ix_knowledge_entry_entities_lookup
ON knowledge_entry_entities(story_id, entity_kind, entity_key, knowledge_kind, source_id);

CREATE TABLE IF NOT EXISTS knowledge_entry_topics (
    story_id TEXT NOT NULL,
    knowledge_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    topic_key TEXT NOT NULL,
    PRIMARY KEY (story_id, knowledge_kind, source_id, topic_key),
    FOREIGN KEY (story_id, knowledge_kind, source_id)
        REFERENCES knowledge_entries(story_id, knowledge_kind, source_id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS ix_knowledge_entry_topics_lookup
ON knowledge_entry_topics(story_id, topic_key, knowledge_kind, source_id);
