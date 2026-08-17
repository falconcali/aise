CREATE TEMP TABLE story_context_simplification_migration_guard (
    value INTEGER CONSTRAINT story_context_simplification_legacy_data_present CHECK (value = 0)
);

INSERT INTO story_context_simplification_migration_guard
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_packs)
UNION ALL
SELECT 1 WHERE EXISTS (SELECT 1 FROM story_instances);

DROP TABLE story_context_simplification_migration_guard;
