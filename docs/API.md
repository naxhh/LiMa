# LIMA API Reference

Base URL: `/api`  
All endpoints use JSON unless stated otherwise.  
No authentication required.

---

## Conventions

### Identifiers
- All primary identifiers are UUID v4 strings
- Folder paths are stored relative to the library root.

### Standard error format
```json
{
  "error": {
    "code": "string",
    "message": "human readable message",
    "details": {}
  }
}
```

### Pagination
Cursor-based pagination parameters:
- `limit`: integer
- `cursor`: opaque string

Paginated response envelope:
```json
{
  "items": [],
  "next_cursor": "string or null"
}
```

### Authentication

Nothing. We may add API tokens in the future from a hard-coded list.

---

## Projects

### List projects
`GET /projects`

Query parameters:
- `query`: full-text search
- `tag`: filter by tag name or tag id
- `collection`: filter by collection id
- `sort`: `name_asc | name_desc | created_desc | updated_desc`
- `limit`, `cursor`

Response (example):
```json
{
  "items": [
    {
      "id": "uuid",
      "name": "Desk Organizer",
      "folder_rel_path": "Desk Organizer",
      "main_image_asset_id": "uuid-or-null",
      "tags": [{"id":"uuid","name":"printable"}],
      "collections": [{"id":"uuid","name":"Workshop"}],
      "asset_counts": {"models": 2, "images": 5, "other": 1},
      "created_at": "2026-01-12T10:00:00Z",
      "updated_at": "2026-01-12T10:05:00Z",
      "last_scanned_at": "2026-01-12T10:05:00Z"
    }
  ],
  "next_cursor": null
}
```

### Get a project
`GET /projects/{project_id}`

Response includes full project metadata and asset listing.

### Create a project
`POST /projects`

Request:
```json
{
  "name": "Project Name",
  "tags": ["tag1", "tag2"],
  "collection_ids": ["uuid"],
  "create_folder": true
}
```

Notes:
- If `create_folder=true`, the server creates the project folder under the library root.
- If `create_folder=false`, this is metadata-only and may be overwritten by a later sync if the folder does not exist.

### Update project metadata
`PATCH /projects/{project_id}`

Request:
```json
{
  "name": "New Name",
  "tag_ids_add": ["uuid"],
  "tag_ids_remove": ["uuid"],
  "collection_ids_add": ["uuid"],
  "collection_ids_remove": ["uuid"],
  "main_image_asset_id": "uuid or null",
  "notes": "string"
}
```

### Delete a project
`DELETE /projects/{project_id}?mode=db_only|delete_files`

Modes:
- `db_only`: delete DB records; keep filesystem folder
- `delete_files`: delete filesystem folder and DB records

---

## Assets

### List assets for a project
`GET /projects/{project_id}/assets`

Query parameters:
- `kind`: `model | image | other`

Response (example):
```json
{
  "items": [
    {
      "id": "uuid",
      "project_id": "uuid",
      "rel_path": "models/organizer.stl",
      "kind": "model",
      "size_bytes": 123456,
      "mtime": "2026-01-12T10:03:11Z",
      "mime": "model/stl",
      "hash_blake3": "hex-or-null"
    }
  ],
  "next_cursor": null
}
```

### Get asset metadata
`GET /assets/{asset_id}`

### Download / stream asset content
`GET /assets/{asset_id}/content`

Behavior:
- Returns raw file bytes.
- Should support `Range` requests for large files.
- Sets `ETag` to `blake3:<hex>` when available (fallbacks allowed).

### Delete an asset
`DELETE /assets/{asset_id}?mode=db_only|delete_file`

Modes:
- `db_only`: delete DB record only
- `delete_file`: delete filesystem file and DB record

---

## Tags

### List tags
`GET /tags`

Query parameters:
- `query`
- `limit`, `cursor`

### Create a tag
`POST /tags`

Request:
```json
{ "name": "printable" }
```

### Rename a tag
`PATCH /tags/{tag_id}`

Request:
```json
{ "name": "3d-printable" }
```

### Delete a tag
`DELETE /tags/{tag_id}`

---

## Collections

### List collections
`GET /collections`

### Create a collection
`POST /collections`

Request:
```json
{ "name": "Workshop" }
```

### Update a collection
`PATCH /collections/{collection_id}`

Request (example):
```json
{ "name": "Workshop - Home" }
```

### Modify projects in a collection
`POST /collections/{collection_id}/projects`

Request:
```json
{
  "add": ["uuid1", "uuid2"],
  "remove": ["uuid3"]
}
```

### Delete a collection
`DELETE /collections/{collection_id}`

---

## Search

### Search projects
`GET /search`

Query parameters:
- `query`
- `tag`
- `collection`
- `limit`, `cursor`

Response is the same envelope as `GET /projects`.

---

## Uploads

### Upload a project (multipart)
`POST /uploads/project` (Content-Type: `multipart/form-data`)

Form fields:
- `name` (string)
- `tags` (repeatable or comma-separated string; pick one convention and document it)
- `collection_ids` (repeatable or comma-separated)
- `main_image` (file, optional)
- `files[]` (file, repeatable)

Response (example):
```json
{ "project_id": "uuid" }
```

---

## Sync & indexing

### Start a sync run
`POST /sync/runs`

Request:
```json
{
  "mode": "full | incremental",
  "reason": "manual | scheduled | watcher",
  "options": {
    "rehash": "none | changed | all",
    "thumbnails": "missing | all",
    "delete_policy": "keep_orphans | mark_orphans | delete_orphans"
  }
}
```

Response:
```json
{ "run_id": "uuid", "status": "queued" }
```

### Get current sync status
`GET /sync/status`

Response (example):
```json
{
  "active_run": {
    "run_id": "uuid",
    "started_at": "2026-01-12T10:00:00Z",
    "phase": "scanning|hashing|thumbnails|db_write|done",
    "progress": { "projects_seen": 120, "assets_seen": 900, "errors": 2 }
  },
  "last_run": {
    "run_id": "uuid",
    "finished_at": "2026-01-12T09:30:00Z",
    "status": "ok",
    "stats": { "projects_added": 3, "projects_updated": 10, "assets_added": 25 }
  }
}
```

### List sync runs
`GET /sync/runs`

Query parameters:
- `limit`, `cursor`

### Get a sync run
`GET /sync/runs/{run_id}`

### Get sync run events
`GET /sync/runs/{run_id}/events`

Query parameters:
- `limit`, `cursor`

Response (example):
```json
{
  "items": [
    {
      "ts": "2026-01-12T10:01:02Z",
      "level": "info",
      "kind": "project_upsert",
      "data": { "path": "Desk Organizer", "project_id": "uuid" }
    }
  ],
  "next_cursor": null
}
```

### Cancel a sync run (optional)
`POST /sync/runs/{run_id}/cancel`

---

## Watcher (optional)

### Get watcher status
`GET /watcher/status`

### Enable watcher
`POST /watcher/enable`

### Disable watcher
`POST /watcher/disable`

---

## System

### Health check
`GET /health`

Response:
```json
{ "ok": true }
```

### Library info
`GET /library/info`

Response (example):
```json
{
  "library_root": "/data/library",
  "free_bytes": 1234567890,
  "project_count": 120,
  "asset_count": 900
}
```
