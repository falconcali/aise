ALTER TABLE memory RENAME TO memory_old;

CREATE TABLE memory (
    id           TEXT PRIMARY KEY,
    character_id TEXT NOT NULL REFERENCES characters(id),
    kind         TEXT NOT NULL,
    content      TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);

INSERT INTO memory (id, character_id, kind, content, created_at)
    SELECT id, character_id, kind, content, created_at FROM memory_old;

DROP TABLE memory_old;
