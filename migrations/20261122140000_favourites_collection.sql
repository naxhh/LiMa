-- Built-in collections are identified by a stable key; user collections have NULL.
ALTER TABLE collections ADD COLUMN system_key TEXT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_collections_system_key ON collections(system_key) WHERE system_key IS NOT NULL;

-- If the user already made a "Favourites" collection, adopt it as the built-in one.
UPDATE collections SET system_key = 'favourites'
WHERE id = (SELECT id FROM collections WHERE name = 'Favourites' COLLATE NOCASE ORDER BY created_at LIMIT 1);
