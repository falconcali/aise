CREATE TABLE story_opening_migration_guard (
    valid INTEGER NOT NULL CONSTRAINT story_opening_migration_ambiguous CHECK (valid = 1)
);

INSERT INTO story_opening_migration_guard (valid)
SELECT CASE
    WHEN EXISTS (
        SELECT 1
        FROM story_packs pack
        WHERE json_type(pack.pack_json, '$.start.role_openings') = 'object'
          AND (
              SELECT COUNT(DISTINCT opening.value)
              FROM json_each(json_extract(pack.pack_json, '$.start.role_openings')) opening
          ) != 1
    ) THEN 0
    ELSE 1
END;

UPDATE story_packs
SET pack_json = json_remove(
    json_set(
        pack_json,
        '$.start.opening',
        (
            SELECT opening.value
            FROM json_each(json_extract(pack_json, '$.start.role_openings')) opening
            LIMIT 1
        )
    ),
    '$.start.role_openings'
)
WHERE json_type(pack_json, '$.start.role_openings') = 'object';

DROP TABLE story_opening_migration_guard;
