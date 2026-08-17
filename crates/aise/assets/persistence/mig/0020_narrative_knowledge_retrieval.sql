ALTER TABLE knowledge_entries ADD COLUMN retrieval_hint TEXT NULL;

UPDATE knowledge_entries
SET retrieval_hint = substr(TRIM(content), 1, 256)
WHERE knowledge_kind IN ('fact', 'rumor')
  AND (retrieval_hint IS NULL OR TRIM(retrieval_hint) = '');

UPDATE knowledge_entries
SET payload_json = json_set(payload_json, '$.value.retrieval_hint', retrieval_hint)
WHERE knowledge_kind IN ('fact', 'rumor')
  AND json_extract(payload_json, '$.value.retrieval_hint') IS NULL;

ALTER TABLE story_instances ADD COLUMN knowledge_id_high_water INTEGER NOT NULL DEFAULT 0;
