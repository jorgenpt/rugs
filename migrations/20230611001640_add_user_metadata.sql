CREATE TABLE IF NOT EXISTS user_events
(
    id            INTEGER PRIMARY KEY NOT NULL,
    project_id    INTEGER NOT NULL,
    change_number INTEGER NOT NULL,
    user_name     TEXT NOT NULL,
    sequence      INTEGER NOT NULL,
    updated_at    DATETIME NOT NULL,
    synced_at     DATETIME,
    vote          INTEGER,
    investigating BOOL,
    starred       BOOL,
    comment       TEXT
);
