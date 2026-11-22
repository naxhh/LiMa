-- Deleted projects waiting in the trash. Their folder sits in <library>/.lima-trash/<id>/
-- and their metadata is kept as a one-project metadata backup, so a restore is complete.
CREATE TABLE trash (
    id          TEXT PRIMARY KEY,
    folder_path TEXT NOT NULL, -- where the project was, relative to the library root
    name        TEXT NOT NULL,
    file_count  INTEGER NOT NULL,
    size_bytes  INTEGER NOT NULL,
    snapshot    TEXT NOT NULL, -- MetadataBackup JSON holding just this project
    trashed_at  TEXT NOT NULL
);
CREATE INDEX trash_trashed_at ON trash (trashed_at);
