-- Asset hashes switch from SHA-256 (bundle uploads) to blake3.
-- Both are 64 hex chars, so clear existing values; the next sync recomputes them.
UPDATE assets SET file_hash = NULL;
