CREATE TABLE IF NOT EXISTS badges_v2
(
    id            INTEGER PRIMARY KEY NOT NULL,
    sequence      INTEGER NOT NULL,
    change_number INTEGER NOT NULL,
    added_at      DATETIME NOT NULL,
    build_type    TEXT NOT NULL,
    result        INTEGER NOT NULL,
    url           TEXT NOT NULL,
    project_id    INTEGER NOT NULL
);

INSERT INTO badges_v2 (id, change_number, added_at, build_type, result, url, project_id, sequence)
    SELECT *, id FROM badges;

DROP TABLE badges;
ALTER TABLE badges_v2 RENAME TO badges;