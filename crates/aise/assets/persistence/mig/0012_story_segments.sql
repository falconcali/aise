CREATE TABLE story_segments (
    id TEXT PRIMARY KEY,
    story_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    origin TEXT NOT NULL CHECK (origin IN ('opening', 'turn')),
    turn_id TEXT REFERENCES story_turns(id) ON DELETE CASCADE,
    story_text TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(story_id, sequence),
    UNIQUE(turn_id),
    CHECK (
        (origin = 'opening' AND turn_id IS NULL)
        OR (origin = 'turn' AND turn_id IS NOT NULL)
    )
);

INSERT INTO story_segments (id, story_id, sequence, origin, turn_id, story_text, created_at)
SELECT 'turn:' || id, world_id, sequence, 'turn', id, story_text, created_at
FROM story_turns;

INSERT INTO story_segments (id, story_id, sequence, origin, turn_id, story_text, created_at)
SELECT
    instance.story_id || ':opening',
    instance.story_id,
    1,
    'opening',
    NULL,
    opening.value,
    instance.created_at_ms
FROM story_instances instance
JOIN story_packs pack ON pack.pack_id = instance.pack_id
JOIN json_each(instance.bindings_json) binding
JOIN json_each(json_extract(pack.pack_json, '$.start.role_openings')) opening ON opening.key = binding.key
WHERE json_extract(binding.value, '$.controller.kind') = 'player'
  AND NOT EXISTS (
      SELECT 1 FROM story_turns turn WHERE turn.world_id = instance.story_id
  )
  AND TRIM(COALESCE(opening.value, '')) != '';
