CREATE TEMP TABLE turn_runtime_alignment_guard (
    value INTEGER CONSTRAINT turn_runtime_alignment_legacy_turn_data CHECK (value = 0)
);

INSERT INTO turn_runtime_alignment_guard
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_turns)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_events)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_segments WHERE origin = 'turn')
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM outbox);

INSERT INTO turn_runtime_alignment_guard
SELECT 1
FROM knowledge_entries
WHERE json_type(source_json, '$.committed_turn.turn_id') = 'text'
   OR json_type(payload_json, '$.value.source.committed_turn.turn_id') = 'text';

INSERT INTO turn_runtime_alignment_guard
SELECT 1
FROM story_instances instance
WHERE EXISTS (
    SELECT 1
    FROM json_each(instance.narrative_state_json, '$.activation_turns') activation
    WHERE activation.type = 'text'
)
OR EXISTS (
    SELECT 1
    FROM json_each(instance.narrative_state_json, '$.pending_effects') effect
    WHERE json_type(effect.value, '$.created_by_turn') = 'text'
);

ALTER TABLE stories ADD COLUMN last_turn_number INTEGER NOT NULL DEFAULT 0
    CHECK (last_turn_number >= 0);

INSERT INTO turn_runtime_alignment_guard
SELECT 1
FROM story_instances instance, json_each(instance.roles_json) role
WHERE role.key GLOB 'role_[0-9]*'
  AND substr(role.key, 6) NOT GLOB '*[^0-9]*'
  AND (
      (length(substr(role.key, 6)) = 4
       AND CAST(substr(role.key, 6) AS INTEGER) BETWEEN 1 AND 9999)
      OR
      (length(substr(role.key, 6)) > 4
       AND substr(role.key, 6, 1) BETWEEN '1' AND '9')
  );

ALTER TABLE story_instances ADD COLUMN role_id_high_water INTEGER NOT NULL DEFAULT 0
    CHECK (role_id_high_water >= 0);

CREATE TABLE story_turns_new (
    story_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    turn_number INTEGER NOT NULL CHECK (turn_number > 0),
    player_input TEXT NOT NULL,
    story_text TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status = 'ok'),
    created_at INTEGER NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    base_revision INTEGER NOT NULL CHECK (base_revision >= 0),
    committed_revision INTEGER NOT NULL CHECK (committed_revision > base_revision),
    result_json TEXT NOT NULL CHECK (json_valid(result_json)),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    PRIMARY KEY (story_id, turn_number),
    UNIQUE (story_id, idempotency_key),
    UNIQUE (story_id, sequence)
);

CREATE TABLE story_events_new (
    id TEXT PRIMARY KEY,
    story_id TEXT NOT NULL,
    turn_number INTEGER NOT NULL,
    seq INTEGER NOT NULL CHECK (seq >= 0),
    kind TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    UNIQUE (story_id, turn_number, seq),
    FOREIGN KEY (story_id, turn_number)
        REFERENCES story_turns_new(story_id, turn_number)
        ON DELETE CASCADE
);

DROP TABLE story_events;
DROP TABLE story_turns;
ALTER TABLE story_turns_new RENAME TO story_turns;
ALTER TABLE story_events_new RENAME TO story_events;

CREATE TABLE story_segments_new (
    id TEXT PRIMARY KEY,
    story_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    origin TEXT NOT NULL CHECK (origin IN ('opening', 'turn')),
    turn_number INTEGER,
    story_text TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (story_id, sequence),
    UNIQUE (story_id, turn_number),
    CHECK (
        (origin = 'opening' AND turn_number IS NULL)
        OR (origin = 'turn' AND turn_number IS NOT NULL AND turn_number > 0)
    ),
    FOREIGN KEY (story_id, turn_number)
        REFERENCES story_turns(story_id, turn_number)
        ON DELETE CASCADE
);

INSERT INTO story_segments_new (id, story_id, sequence, origin, turn_number, story_text, created_at)
SELECT id, story_id, sequence, 'opening', NULL, story_text, created_at
FROM story_segments
WHERE origin = 'opening';

DROP TABLE story_segments;
ALTER TABLE story_segments_new RENAME TO story_segments;

CREATE TABLE outbox_new (
    id TEXT PRIMARY KEY,
    story_id TEXT NOT NULL,
    turn_number INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    attempt_count INTEGER NOT NULL,
    published_at INTEGER,
    last_error TEXT
);

DROP TABLE outbox;
ALTER TABLE outbox_new RENAME TO outbox;

CREATE INDEX IF NOT EXISTS idx_outbox_unpublished
    ON outbox(created_at)
    WHERE published_at IS NULL;

PRAGMA foreign_key_check;

DROP TABLE turn_runtime_alignment_guard;
