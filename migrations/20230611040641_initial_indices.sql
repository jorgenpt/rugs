CREATE INDEX project_index ON projects (stream, project);
CREATE INDEX badge_project_sequence_change ON badges (project_id, sequence, change_number);
CREATE INDEX user_event_project_sequence_change ON user_events (project_id, sequence, change_number);
CREATE INDEX user_event_specific_change ON user_events(project_id, user_name, change_number);