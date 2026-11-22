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
    "details": {},
    "request_id": "string"
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

## API Endpoints

The authoritative OpenAPI spec is served at `GET /openapi.json` and browsable at `/docs` (Swagger UI), both outside `/api`. This file is a summary.

### Health

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | Health check (DB ping) |

### Stats

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/stats` | Library totals (projects, files, bytes) |

### Projects

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/projects` | Create project (metadata only) |
| GET | `/projects` | List projects (cursor pagination + search). Each item includes file counts, total size, model formats, tags and `preview_id` (a model whose thumbnail stands in when there's no image). Search matches word prefixes. Filters: `tags` (comma-separated tag ids, all must match), `formats` (comma-separated model extensions, any matches), `no_image=true`. Order: `sort` = `updated` (default) / `created` / `modified` (newest file date) / `name` / `size` / `files`, `order` = `asc` / `desc`; ignored when searching |
| DELETE | `/projects/{project_id}` | Delete a project: it goes to the trash (`{trash_id}` in the response) unless the trash is off. `?files=keep` only removes it from LiMa: the folder stays on disk and is added to the ignore list |
| GET | `/projects/{project_id}` | Get single project details |
| GET | `/projects/{project_id}/download` | All of the project's files as a streamed zip |
| POST | `/projects/download` | Several projects in one zip, one folder each (form field `ids`, comma-separated, up to 500) |
| POST | `/projects/bulk` | One action on many projects: `{project_ids, action}` with `action` = `add_tags`/`remove_tags` (+`tags`), `add_to_collection`/`remove_from_collection` (+`collection_id`), `favourite`, `unfavourite`, `delete` (to the trash), `remove_keep_files`. Returns `{affected, trash_ids, failed}` |
| PUT | `/projects/{project_id}/favourite` | Add the project to Favourites |
| DELETE | `/projects/{project_id}/favourite` | Remove the project from Favourites |
| PATCH | `/projects/{project_id}` | Update project metadata (name, description, main image) |
| POST | `/projects/{project_id}/import` | Import bundle into project (move files, create assets, set main image) |

### Assets

| Method | Path | Purpose |
|--------|------|---------|
| DELETE | `/projects/{project_id}/assets/{asset_id}` | Remove asset |

### Tags

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/tags` | List tags |
| POST | `/tags` | Create tag |
| PATCH | `/tags/{tag_id}` | Rename / recolor tag |
| DELETE | `/tags/{tag_id}` | Delete tag (unlinks it from projects) |

### Collections

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/collections` | List collections (Favourites first) with project count and up to 4 cover images. Favourites is built in: it always exists and can't be renamed or deleted |
| POST | `/collections` | Create collection (`name`, optional `description`; names unique ignoring case) |
| GET | `/collections/{collection_id}` | Collection with its projects. `sort` = `added` (default, when added to the collection) or any project sort, plus `order` |
| PATCH | `/collections/{collection_id}` | Update name and/or description |
| DELETE | `/collections/{collection_id}` | Delete collection (projects are kept) |
| POST | `/collections/{collection_id}/projects` | Add project (`{project_id}`; adding twice is a no-op) |
| DELETE | `/collections/{collection_id}/projects/{project_id}` | Remove project from collection |

### Bundles

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/bundles` | Create a new bundle and upload files (multipart) |
| DELETE | `/bundles/{bundle_id}` | Delete bundle and staged files |

### Imports

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/imports/preview` | Read a model page (`{url}`): name, description, tags, images, files. Downloads nothing |
| POST | `/imports` | Create the project now and download the chosen `file_ids` (+ images) in the background. `401 bambu_login_required` when a MakerWorld download has no working Bambu token |
| GET | `/imports/{job_id}` | Import progress (`downloading`, `importing`, `complete`, `failed`). A failed import removes its project. Jobs live in memory for an hour after they finish |
| GET | `/integrations/bambu/status` | Whether this browser's Bambu token works |
| POST | `/integrations/bambu/login` | Start a Bambu login (`{account, password}`) → `logged_in`, `code_sent` or `tfa_required` |
| POST | `/integrations/bambu/login/send-code` | Email a login code with no password (`{account}`), for Google/Apple sign-in accounts |
| POST | `/integrations/bambu/login/code` | Finish with the emailed code (`{account, code}`) |
| POST | `/integrations/bambu/login/tfa` | Finish with an authenticator code (`{tfa_key, code}`) |
| POST | `/integrations/bambu/token` | Use a token pasted from makerworld.com's `token` cookie |
| POST | `/integrations/bambu/logout` | Drop the token cookie |

### Sync / Maintenance

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/sync/run` | Trigger filesystem sync (`incremental` default, `full` re-hashes everything) |
| GET | `/sync/status` | Current/last sync status |
| GET | `/sync/runs` | List sync runs |
| GET | `/sync/runs/{run_id}` | Get sync run details |
| GET | `/sync/runs/{run_id}/events` | Sync event log |
| GET | `/ignored-folders` | Folders removed from LiMa with their files kept; sync skips them |
| DELETE | `/ignored-folders?folder_path=…` | Un-ignore a folder and sync it right away |

### Trash

Deleted projects move to `<library>/.lima-trash/<id>/` with their metadata kept, and are purged after
`LIMA_TRASH_DAYS` (default 30).

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/trash` | Trashed projects (newest first) and the retention in days |
| POST | `/trash/{trash_id}/restore` | Move the folder back, re-index it and reapply tags, collections, description and cover. A taken folder name comes back as `<name>-restored` |
| DELETE | `/trash/{trash_id}` | Delete one item for good |
| DELETE | `/trash` | Empty the trash |

### Duplicates

Found by file hash, nothing is changed until you pick one to keep. Project groups are projects
whose model files are the same, or at least 80% the same. Shared files are the same file in
projects that aren't copies of each other (shown for reference, no action). Files are never
deduplicated on disk.

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/duplicates` | Project groups (suggested keeper first) and shared files, biggest first, with reclaimable bytes |
| POST | `/duplicates/resolve` | `{keep_id, remove_ids, remove: "trash" \| "keep_files"}`: merge the others' tags, collections and favourite into the kept project (and description, source and cover where it has none), then trash them or remove them from LiMa |
| POST | `/duplicates/dismiss` | `{key}`: hide a group as "not duplicates" until it changes |
| DELETE | `/duplicates/dismissed` | Show every hidden group again |

### Metadata backup

Backups are JSON (`format: "lima-metadata"`, `version: 1`) keyed by project folder, with file
hashes so renamed folders can be matched. Restores replace names, descriptions, covers and
sources, and add tags, collections and favourites (nothing is removed). Ignored folders in the
backup are ignored again, and dropped from the index if a sync had brought them back.

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/metadata/export` | Download all metadata as JSON |
| POST | `/metadata/restore` | Restore from an uploaded backup (`?dry_run=true` previews) |
| GET | `/metadata/backups` | Stored backups, folder and schedule |
| POST | `/metadata/backups` | Write a backup now |
| GET | `/metadata/backups/{name}` | Download a stored backup |
| POST | `/metadata/backups/{name}/restore` | Restore a stored backup (`?dry_run=true` previews) |

### Files

Served outside `/api`, straight from disk.

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/media/library/{folder}/{file}` | A project file from the library |
| GET | `/media/thumbs/{project_id}/{asset_id}.jpg` | Generated thumbnail: images downscaled; models (3MF/STL/OBJ) use the slicer's embedded picture or a render |
