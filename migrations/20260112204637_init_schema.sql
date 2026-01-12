-- Initial schema for Lima app database.
-- PRAGMA foreign_keys = ON;
-- PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY,                       -- UUID
  folder_path TEXT NOT NULL UNIQUE,          -- relative path from library root
  `name` TEXT NOT NULL,                      -- default: folder_path
  `description` TEXT NOT NULL DEFAULT '',
  main_image_id TEXT NULL,                   -- FK -> assets(id)
  created_at TEXT NOT NULL,                  -- RFC3339
  updated_at TEXT NOT NULL,                  -- RFC3339
  last_scanned_at TEXT NULL                  -- RFC3339
);

CREATE TABLE IF NOT EXISTS assets (
  id TEXT PRIMARY KEY,                      -- UUID
  project_id TEXT NOT NULL,
  file_path TEXT NOT NULL,                  -- relative to project folder
  kind TEXT NOT NULL CHECK (kind IN ('model','image','other')),
  size_bytes INTEGER NOT NULL,
  mtime TEXT NOT NULL,                      -- RFC3339 (from fs metadata)
  mime TEXT NOT NULL DEFAULT '',
  file_hash TEXT NULL,                      -- hex; null until computed
  created_at TEXT NOT NULL,                 -- RFC3339
  updated_at TEXT NOT NULL,                 -- RFC3339
  UNIQUE(project_id, file_path),
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_assets_project_kind ON assets(project_id, kind);

CREATE TABLE IF NOT EXISTS tags (
  id TEXT PRIMARY KEY,                      -- UUID
  `name` TEXT NOT NULL UNIQUE,
  color TEXT NOT NULL,                     -- hex color code, e.g., #RRGGBB
  created_at TEXT NOT NULL,                 -- RFC3339
  updated_at TEXT NOT NULL                  -- RFC3339
);

CREATE TABLE IF NOT EXISTS project_tags (
  project_id TEXT NOT NULL,
  tag_id TEXT NOT NULL,
  PRIMARY KEY(project_id, tag_id),
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_project_tags_tag ON project_tags(tag_id);

CREATE TABLE IF NOT EXISTS collections (
  id TEXT PRIMARY KEY,                      -- UUID
  `name` TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,                 -- RFC3339
  updated_at TEXT NOT NULL                  -- RFC3339
);

CREATE TABLE IF NOT EXISTS collection_projects (
  collection_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  -- position INTEGER NOT NULL DEFAULT 0, -- TODO: I think I prefer order by added date rather than static position.
  created_at TEXT NOT NULL,                 -- RFC3339
  PRIMARY KEY(collection_id, project_id),
  FOREIGN KEY(collection_id) REFERENCES collections(id) ON DELETE CASCADE,
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_collection_projects_project ON collection_projects(project_id); -- TODO: unsure if will need this one
-- TODO: check if we need idx for created_at sorting.

CREATE TABLE IF NOT EXISTS sync_runs (
  id TEXT PRIMARY KEY,                      -- UUID
  mode TEXT NOT NULL CHECK (mode IN ('full','incremental')),
  reason TEXT NOT NULL CHECK (reason IN ('manual','scheduled','watcher')),
  `status` TEXT NOT NULL CHECK (`status` IN ('queued','running','complete','failed','cancelled')),
  options_json TEXT NOT NULL DEFAULT '{}',
  stats_json TEXT NOT NULL DEFAULT '{}',
  started_at TEXT NULL,                    -- RFC3339
  finished_at TEXT NULL,                   -- RFC3339
  error TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_sync_runs_started_at ON sync_runs(started_at);

CREATE TABLE IF NOT EXISTS sync_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,                     -- FK -> sync_runs(id)
  created_at TEXT NOT NULL,                 -- RFC3339
  `level` TEXT NOT NULL CHECK (`level` IN ('trace','debug','info','warn','error')),
  kind TEXT NOT NULL,                       -- e.g., project_upsert, hash_failed, etc.
  data_json TEXT NOT NULL DEFAULT '{}',
  FOREIGN KEY(run_id) REFERENCES sync_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_sync_events_run_id ON sync_events(run_id);
CREATE INDEX IF NOT EXISTS idx_sync_events_ts ON sync_events(created_at);


-- ---------- Full-text search (FTS5) ----------
-- projects_fts indexes: project name + tags (concatenated)
CREATE VIRTUAL TABLE IF NOT EXISTS projects_fts
USING fts5(
  project_id UNINDEXED,
  `name`,
  tags,
  content=''
);

-- Helper view for tags concatenation (used by triggers)
CREATE VIEW IF NOT EXISTS v_project_tags_text AS
SELECT
  p.id AS project_id,
  COALESCE(GROUP_CONCAT(t.name, ' '), '') AS tags_text
FROM projects p
LEFT JOIN project_tags pt ON pt.project_id = p.id
LEFT JOIN tags t ON t.id = pt.tag_id
GROUP BY p.id;

-- Rebuild FTS row for a single project_id (used in triggers)
-- SQLite doesn't support stored procedures; we inline logic in triggers.

-- Keep FTS in sync on projects changes
CREATE TRIGGER IF NOT EXISTS trg_projects_ai AFTER INSERT ON projects BEGIN
  INSERT INTO projects_fts(project_id, name, tags)
  SELECT
    NEW.id,
    NEW.name,
    (SELECT tags_text FROM v_project_tags_text WHERE project_id = NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS trg_projects_au AFTER UPDATE OF name ON projects BEGIN
  DELETE FROM projects_fts WHERE project_id = NEW.id;
  INSERT INTO projects_fts(project_id, name, tags)
  SELECT
    NEW.id,
    NEW.name,
    (SELECT tags_text FROM v_project_tags_text WHERE project_id = NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS trg_projects_ad AFTER DELETE ON projects BEGIN
  DELETE FROM projects_fts WHERE project_id = OLD.id;
END;

-- Keep FTS tags field in sync when tags mappings change
CREATE TRIGGER IF NOT EXISTS trg_project_tags_ai AFTER INSERT ON project_tags BEGIN
  DELETE FROM projects_fts WHERE project_id = NEW.project_id;
  INSERT INTO projects_fts(project_id, name, tags)
  SELECT
    p.id,
    p.name,
    (SELECT tags_text FROM v_project_tags_text WHERE project_id = p.id)
  FROM projects p
  WHERE p.id = NEW.project_id;
END;

CREATE TRIGGER IF NOT EXISTS trg_project_tags_ad AFTER DELETE ON project_tags BEGIN
  DELETE FROM projects_fts WHERE project_id = OLD.project_id;
  INSERT INTO projects_fts(project_id, name, tags)
  SELECT
    p.id,
    p.name,
    (SELECT tags_text FROM v_project_tags_text WHERE project_id = p.id)
  FROM projects p
  WHERE p.id = OLD.project_id;
END;

-- Keep FTS tags updated when a tag is renamed
CREATE TRIGGER IF NOT EXISTS trg_tags_au AFTER UPDATE OF name ON tags BEGIN
  -- Rebuild FTS for all projects that use this tag
  DELETE FROM projects_fts
  WHERE project_id IN (SELECT project_id FROM project_tags WHERE tag_id = NEW.id);

  INSERT INTO projects_fts(project_id, name, tags)
  SELECT
    p.id,
    p.name,
    (SELECT tags_text FROM v_project_tags_text WHERE project_id = p.id)
  FROM projects p
  WHERE p.id IN (SELECT project_id FROM project_tags WHERE tag_id = NEW.id);
END;
