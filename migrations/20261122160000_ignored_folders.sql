-- Project folders removed from LiMa but kept on disk. Sync skips them.
CREATE TABLE ignored_folders (
    folder_path TEXT PRIMARY KEY, -- relative to the library root, like projects.folder_path
    created_at  TEXT NOT NULL
);
