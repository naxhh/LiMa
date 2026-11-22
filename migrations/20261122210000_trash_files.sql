-- Single files can be trashed too: `file_path` (relative to the project folder) is set,
-- `folder_path` is the project's folder and `name` the project's name.
ALTER TABLE trash ADD COLUMN file_path TEXT NULL;
