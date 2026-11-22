-- Duplicate groups the user marked as "not duplicates". The key names the exact group
-- (its projects, or a file hash), so a group that changes shows up again.
CREATE TABLE duplicate_dismissals (
    key        TEXT PRIMARY KEY,
    created_at TEXT NOT NULL
);
