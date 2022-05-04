CREATE TABLE IF NOT EXISTS projects
(
  project_id INTEGER PRIMARY KEY NOT NULL,
  project  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS badges
(
    id            INTEGER PRIMARY KEY NOT NULL,
    change_number INTEGER NOT NULL,
    added_at      DATETIME NOT NULL,
    build_type    TEXT NOT NULL,
    result        INTEGER NOT NULL,
    url           TEXT NOT NULL,
    project_id    INTEGER NOT NULL,
    archive_path  TEXT
);