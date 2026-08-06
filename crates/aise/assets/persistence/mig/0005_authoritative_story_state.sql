ALTER TABLE stories ADD COLUMN story_instructions TEXT NOT NULL DEFAULT '';
ALTER TABLE stories ADD COLUMN story_config TEXT NOT NULL DEFAULT '{"style":null,"point_of_view":null,"tense":null}';
ALTER TABLE stories ADD COLUMN current_scene TEXT NOT NULL DEFAULT '{"text":""}';
ALTER TABLE stories ADD COLUMN story_summary TEXT NOT NULL DEFAULT '{"text":""}';
ALTER TABLE stories ADD COLUMN active_constraints TEXT NOT NULL DEFAULT '[]';
