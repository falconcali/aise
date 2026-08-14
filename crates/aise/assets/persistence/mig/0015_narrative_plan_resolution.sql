CREATE TABLE story_instances_new (
    story_id TEXT PRIMARY KEY REFERENCES stories(id) ON DELETE CASCADE,
    pack_id TEXT NOT NULL REFERENCES story_packs(pack_id),
    settings_json TEXT NOT NULL CHECK (json_valid(settings_json)),
    bindings_json TEXT NOT NULL CHECK (json_valid(bindings_json)),
    characters_json TEXT NOT NULL CHECK (json_valid(characters_json)),
    relationships_json TEXT NOT NULL CHECK (json_valid(relationships_json)),
    narrative_state_json TEXT NOT NULL CHECK (json_valid(narrative_state_json)),
    fact_values_json TEXT NOT NULL CHECK (json_valid(fact_values_json)),
    created_at_ms INTEGER NOT NULL
);

INSERT INTO story_instances_new (
    story_id, pack_id, settings_json, bindings_json, characters_json,
    relationships_json, narrative_state_json, fact_values_json, created_at_ms
)
SELECT story_id, pack_id, settings_json, bindings_json, characters_json,
       relationships_json, narrative_state_json, '{}', created_at_ms
FROM story_instances;

DROP TABLE story_instances;
ALTER TABLE story_instances_new RENAME TO story_instances;

PRAGMA foreign_key_check;
