ALTER TABLE badges DROP COLUMN archive_path;

CREATE TABLE projects_v2
(
    project_id INTEGER PRIMARY KEY NOT NULL,
    stream    TEXT NOT NULL COLLATE NOCASE,
    project    TEXT NOT NULL COLLATE NOCASE,
    first_slash INTEGER,
    second_slash INTEGER
);

INSERT INTO projects_v2 (project_id, project, stream) SELECT *, "" FROM projects;

UPDATE projects_v2
    SET first_slash = instr(substring(project, 3), "/") + 3;
UPDATE projects_v2
    SET second_slash = instr(substring(project, first_slash), "/") + first_slash;

UPDATE projects_v2
    SET 
        stream = lower(substring(project, 1, second_slash - 2)), 
        project = lower(substring(project, second_slash));

ALTER TABLE projects_v2 DROP COLUMN first_slash;
ALTER TABLE projects_v2 DROP COLUMN second_slash;

DROP TABLE projects;
ALTER TABLE projects_v2 RENAME TO projects;