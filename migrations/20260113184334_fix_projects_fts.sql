DROP TRIGGER IF EXISTS trg_projects_ai;
DROP TRIGGER IF EXISTS trg_projects_au;
DROP TRIGGER IF EXISTS trg_projects_ad;
DROP TRIGGER IF EXISTS trg_project_tags_ai;
DROP TRIGGER IF EXISTS trg_project_tags_ad;
DROP TRIGGER IF EXISTS trg_tags_au;

DROP VIEW IF EXISTS v_project_tags_text;

DROP TABLE IF EXISTS projects_fts;

CREATE VIEW IF NOT EXISTS v_project_tags_text AS
SELECT
  p.id AS project_id,
  COALESCE(GROUP_CONCAT(t.name, ' '), '') AS tags_text
FROM projects p
LEFT JOIN project_tags pt ON pt.project_id = p.id
LEFT JOIN tags t ON t.id = pt.tag_id
GROUP BY p.id;

CREATE VIRTUAL TABLE IF NOT EXISTS projects_fts
USING fts5(
  project_id UNINDEXED,
  name,
  tags
);

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

CREATE TRIGGER IF NOT EXISTS trg_tags_au AFTER UPDATE OF name ON tags BEGIN
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

INSERT INTO projects_fts(project_id, name, tags)
SELECT
  p.id,
  p.name,
  (SELECT tags_text FROM v_project_tags_text WHERE project_id = p.id)
FROM projects p;
