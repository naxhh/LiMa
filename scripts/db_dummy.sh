sqlite3 data/state/lima.db <<'SQL'
INSERT INTO projects(id, folder_path, name, description, created_at, updated_at)
VALUES ('p1','Demo','Demo','', '2026-01-12T00:00:00Z','2026-01-12T00:00:00Z');

INSERT INTO tags(id, name, color, created_at, updated_at)
VALUES ('t1','printable','#FF00FF','2026-01-12T00:00:00Z','2026-01-12T00:00:00Z');

INSERT INTO tags(id, name, color, created_at, updated_at)
VALUES ('t2','office','#00FFFF','2026-01-12T00:00:00Z','2026-01-12T00:00:00Z');

INSERT INTO project_tags(project_id, tag_id) VALUES ('p1','t1');
INSERT INTO project_tags(project_id, tag_id) VALUES ('p1','t2');

SELECT project_id, name, tags FROM projects_fts;
SELECT project_id FROM projects_fts WHERE projects_fts MATCH 'printable';
SQL
