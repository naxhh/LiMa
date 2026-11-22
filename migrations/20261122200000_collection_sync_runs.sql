-- History of collection imports and syncs (kept when the collection is deleted).
CREATE TABLE collection_sync_runs (
    id              TEXT PRIMARY KEY,
    collection_id   TEXT NOT NULL,
    collection_name TEXT NOT NULL,
    source          TEXT NOT NULL,
    reason          TEXT NOT NULL,            -- import | manual | scheduled
    status          TEXT NOT NULL,            -- running | complete | failed
    started_at      TEXT NOT NULL,
    finished_at     TEXT NULL,
    imported        INTEGER NOT NULL DEFAULT 0,
    linked          INTEGER NOT NULL DEFAULT 0,
    removed         INTEGER NOT NULL DEFAULT 0,
    failures_json   TEXT NOT NULL DEFAULT '[]', -- [{name, error}]
    error           TEXT NULL
);
CREATE INDEX idx_collection_sync_runs_started ON collection_sync_runs(started_at DESC, id DESC);
