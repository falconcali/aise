ALTER TABLE story_turns RENAME TO story_turns_old;
ALTER TABLE story_events RENAME TO story_events_old;

CREATE TABLE story_turns (
    id              TEXT PRIMARY KEY,
    world_id        TEXT NOT NULL,
    player_input    TEXT NOT NULL,
    story_text      TEXT NOT NULL,
    summary_delta   TEXT,
    status          TEXT NOT NULL,
    created_at      INTEGER NOT NULL
);

INSERT INTO story_turns (id, world_id, player_input, story_text, summary_delta, status, created_at)
    SELECT id, world_id, player_input, story_text, summary_delta, status, created_at FROM story_turns_old;

CREATE TABLE story_events (
    id          TEXT PRIMARY KEY,
    turn_id     TEXT NOT NULL REFERENCES story_turns(id),
    seq         INTEGER NOT NULL,
    kind        TEXT NOT NULL,
    payload     TEXT NOT NULL,
    UNIQUE (turn_id, seq)
);

INSERT INTO story_events (id, turn_id, seq, kind, payload)
    SELECT id, turn_id, seq, kind, payload FROM story_events_old;

DROP TABLE story_events_old;
DROP TABLE story_turns_old;
