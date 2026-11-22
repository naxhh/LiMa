# LIMA API Reference

NOTE: this document changes a lot. So it's likely that this doesn't reflect reality. Use openapi files for that.

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

## Scope & Roadmap

---

### Legend

- ✅ **Done** — Implemented and present in OpenAPI
- 🟡 **Planned (v0)** — In scope for v0
- ⛔ **Out of scope (v0)** — Explicitly excluded for now
- 🔵 **Post-v0** — Lower priority / later milestone

---

### Health

| Method | Path | Purpose | Status |
|------|------|---------|--------|
| GET | `/health` | Health check (DB ping) | ✅ Done |

---

### Bundles (Staging uploads)

| Method | Path | Purpose | Status |
|------|------|---------|--------|
| POST | `/bundles` | Create a new bundle and upload files (multipart) | ✅ Done |
| POST | `/bundles/{bundle_id}/files` | Append files to existing bundle | ⛔ Out of scope |
| GET | `/bundles/{bundle_id}` | Inspect bundle metadata and files | ⛔ Out of scope |
| DELETE | `/bundles/{bundle_id}` | Delete bundle and staged files | ✅ Done |

---

### Projects (Core v0 focus)

| Method | Path | Purpose | Status |
|------|------|---------|--------|
| POST | `/projects` | Create project (metadata only) | ✅ Done |
| GET | `/projects` | List projects (cursor pagination + search). Each item includes file counts, total size, model formats and tags. Search matches word prefixes. Filters: `tags` (comma-separated tag ids, all must match), `formats` (comma-separated model extensions, any matches), `no_image=true` | ✅ Done |
| GET | `/stats` | Library totals (projects, files, bytes) | ✅ Done |
| DELETE | `/projects/{project_id}` | Delete project (DB + filesystem) | ✅ Done |
| GET | `/projects/{project_id}` | Get single project details | ✅ Done |
| GET | `/projects/{project_id}/download` | All of the project's files as a streamed zip | ✅ Done |
| PUT | `/projects/{project_id}/favourite` | Add the project to Favourites | ✅ Done |
| DELETE | `/projects/{project_id}/favourite` | Remove the project from Favourites | ✅ Done |
| PATCH | `/projects/{project_id}` | Update project metadata (name, description, main image) | ✅ Done |
| POST | `/projects/{project_id}/imports` | Import bundle into project (move files, create assets, set main image) | ✅ Done |
| GET | `/projects/{project_id}/assets` | List all assets for the project | ⛔ Out of scope (v0) |
| DELETE | `/projects/{project_id}/assets/{asset_id}` | Remove asset | ✅ Done |

---

### Tags (v0 after projects)

| Method | Path | Purpose | Status |
|------|------|---------|--------|
| GET | `/tags` | List tags | ✅ Done |
| POST | `/tags` | Create tag | ✅ Done |
| PATCH | `/tags/{tag_id}` | Rename / recolor tag | ✅ Done |
| DELETE | `/tags/{tag_id}` | Delete tag (unlinks it from projects) | ✅ Done |

---

### Collections

| Method | Path | Purpose | Status |
|------|------|---------|--------|
| GET | `/collections` | List collections (Favourites first) with project count and up to 4 cover images. Favourites is built in: it always exists and can't be renamed or deleted | ✅ Done |
| POST | `/collections` | Create collection (`name`, optional `description`; names unique ignoring case) | ✅ Done |
| GET | `/collections/{collection_id}` | Collection with its projects (newest added first) | ✅ Done |
| PATCH | `/collections/{collection_id}` | Update name and/or description | ✅ Done |
| DELETE | `/collections/{collection_id}` | Delete collection (projects are kept) | ✅ Done |
| POST | `/collections/{collection_id}/projects` | Add project (`{project_id}`; adding twice is a no-op) | ✅ Done |
| DELETE | `/collections/{collection_id}/projects/{project_id}` | Remove project from collection | ✅ Done |

---

### URL imports (Printables, MakerWorld)

| Method | Path | Purpose | Status |
|------|------|---------|--------|
| POST | `/imports/preview` | Read a model page (`{url}`): name, description, tags, images, files. Downloads nothing | ✅ Done |
| POST | `/imports` | Create the project now and download the chosen `file_ids` (+ images) in the background. `401 bambu_login_required` when a MakerWorld download has no working Bambu token | ✅ Done |
| GET | `/imports/{job_id}` | Import progress (`downloading`, `importing`, `complete`, `failed`). A failed import removes its project. Jobs live in memory for an hour after they finish | ✅ Done |
| GET | `/integrations/bambu/status` | Whether this browser's Bambu token works | ✅ Done |
| POST | `/integrations/bambu/login` | Start a Bambu login (`{account, password}`) → `logged_in`, `code_sent` or `tfa_required` | ✅ Done |
| POST | `/integrations/bambu/login/send-code` | Email a login code with no password (`{account}`), for Google/Apple sign-in accounts | ✅ Done |
| POST | `/integrations/bambu/login/code` | Finish with the emailed code (`{account, code}`) | ✅ Done |
| POST | `/integrations/bambu/login/tfa` | Finish with an authenticator code (`{tfa_key, code}`) | ✅ Done |
| POST | `/integrations/bambu/token` | Use a token pasted from makerworld.com's `token` cookie | ✅ Done |
| POST | `/integrations/bambu/logout` | Drop the token cookie | ✅ Done |

Printables works without an account. MakerWorld metadata is public, but downloads
need a Bambu account token. Lima never stores the token: the login endpoints set it as an
`HttpOnly; SameSite=Strict` cookie named `lima_bambu_token`, scoped to `/api`, and `Secure` when
`X-Forwarded-Proto: https`. It expires with the token, or after 30 days when the token doesn't say.
Each browser logs in on its own. Imported projects record `source_url`, `source_author` and
`source_license`.

---

### Sync / Maintenance

| Method | Path | Purpose | Status |
|------|------|---------|--------|
| POST | `/sync/run` | Trigger filesystem sync (`incremental` default, `full` re-hashes everything) | ✅ Done |
| GET | `/sync/status` | Current/last sync status | ✅ Done |
| GET | `/sync/runs` | List sync runs | ✅ Done |
| GET | `/sync/runs/{run_id}` | Get sync run details | ✅ Done |
| GET | `/sync/runs/{run_id}/events` | Sync event log | ✅ Done |

### Metadata backup

Backups are JSON (`format: "lima-metadata"`, `version: 1`) keyed by project folder, with file
hashes so renamed folders can be matched. Restores replace names, descriptions, covers and
sources, and add tags, collections and favourites (nothing is removed).

| Method | Path | Purpose | Status |
|------|------|---------|--------|
| GET | `/metadata/export` | Download all metadata as JSON | ✅ Done |
| POST | `/metadata/restore` | Restore from an uploaded backup (`?dry_run=true` previews) | ✅ Done |
| GET | `/metadata/backups` | Stored backups, folder and schedule | ✅ Done |
| POST | `/metadata/backups` | Write a backup now | ✅ Done |
| GET | `/metadata/backups/{name}` | Download a stored backup | ✅ Done |
| POST | `/metadata/backups/{name}/restore` | Restore a stored backup (`?dry_run=true` previews) | ✅ Done |
