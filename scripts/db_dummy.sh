sqlite3 data/state/lima.db <<'SQL'
DELETE FROM projects;
DELETE FROM tags;
DELETE FROM project_tags;

INSERT INTO projects(id, folder_path, name, description, created_at, updated_at)
VALUES
('p1','Demo1','Hasta que ya no este','', '2026-01-12T00:00:00Z','2026-01-12T00:00:00Z'),
('p2','Demo2','Modelo juego de mesa','', '2026-01-13T00:00:00Z','2026-01-12T00:00:00Z'),
('p3','Demo3','Tray dados','', '2026-01-14T00:00:00Z','2026-01-12T00:00:00Z'),
('p4','Demo4','Da dos o tres','', '2026-01-15T00:00:00Z','2026-01-12T00:00:00Z'),
('p5','Demo5','Test 1 2 3','', '2026-01-16T00:00:00Z','2026-01-12T00:00:00Z'),
('p6','Demo6','Alfajores zápato','', '2026-01-17T00:00:00Z','2026-01-12T00:00:00Z'),
('p7','Demo7','eñe leñe','', '2026-01-18T00:00:00Z','2026-01-12T00:00:00Z'),
('p8','Demo8','Demo 8','', '2026-01-19T00:00:00Z','2026-01-12T00:00:00Z')
;



INSERT INTO tags(id, name, color, created_at, updated_at)
VALUES ('t1','printable','#FF00FF','2026-01-12T00:00:00Z','2026-01-12T00:00:00Z');

INSERT INTO tags(id, name, color, created_at, updated_at)
VALUES ('t2','office','#00FFFF','2026-01-12T00:00:00Z','2026-01-12T00:00:00Z');

INSERT INTO project_tags(project_id, tag_id) VALUES ('p1','t1');
INSERT INTO project_tags(project_id, tag_id) VALUES ('p1','t2');

SELECT project_id, name, tags FROM projects_fts;
SELECT project_id FROM projects_fts WHERE projects_fts MATCH 'printable';
SQL
