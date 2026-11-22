-- STEP files are models (they were indexed as "other", so the STEP filter never matched them).
UPDATE assets SET kind = 'model'
WHERE kind = 'other' AND (lower(file_path) LIKE '%.step' OR lower(file_path) LIKE '%.stp');
