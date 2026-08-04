ALTER TABLE characters RENAME TO characters_old;

CREATE TABLE characters (
    id          TEXT PRIMARY KEY,
    world_id    TEXT NOT NULL,
    state       TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

INSERT INTO characters (id, world_id, state, created_at, updated_at)
    SELECT id, world_id, state, created_at, updated_at FROM characters_old;

DROP TABLE characters_old;
