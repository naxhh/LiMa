-- Where a project was imported from (Printables, MakerWorld, ...). NULL for local projects.
ALTER TABLE projects ADD COLUMN source_url TEXT NULL;
ALTER TABLE projects ADD COLUMN source_author TEXT NULL;
ALTER TABLE projects ADD COLUMN source_license TEXT NULL;
