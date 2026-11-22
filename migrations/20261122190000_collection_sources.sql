-- A LiMa collection imported from a site's collection, optionally kept in sync with it.
CREATE TABLE collection_sources (
    id             TEXT PRIMARY KEY,
    collection_id  TEXT NOT NULL UNIQUE REFERENCES collections(id) ON DELETE CASCADE,
    source         TEXT NOT NULL,               -- makerworld | thingiverse
    source_id      TEXT NOT NULL,
    url            TEXT NOT NULL,
    keep_in_sync   INTEGER NOT NULL DEFAULT 0,
    last_synced_at TEXT NULL,
    last_error     TEXT NULL,
    created_at     TEXT NOT NULL,
    UNIQUE(source, source_id)
);

-- Every model seen in a source collection.
CREATE TABLE collection_source_items (
    source_ref TEXT NOT NULL REFERENCES collection_sources(id) ON DELETE CASCADE,
    model_id   TEXT NOT NULL,                   -- the model's id on the site
    model_url  TEXT NOT NULL,
    name       TEXT NOT NULL,
    project_id TEXT NULL REFERENCES projects(id) ON DELETE SET NULL,
    -- 1 when this collection created the project: it's deleted (to the trash) if the model
    -- leaves the source collection. Projects that were already in the library are only
    -- taken out of the LiMa collection.
    imported   INTEGER NOT NULL DEFAULT 0,
    -- 1 when the user chose not to import it; sync leaves it alone.
    excluded   INTEGER NOT NULL DEFAULT 0,
    last_error TEXT NULL,
    PRIMARY KEY(source_ref, model_id)
);

-- Credentials the server keeps for background work (the Bambu token for synced
-- MakerWorld collections).
CREATE TABLE integration_tokens (
    name       TEXT PRIMARY KEY,
    token      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
