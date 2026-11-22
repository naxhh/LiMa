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
| GET | `/projects` | List projects (cursor pagination + search) | ✅ Done |
| DELETE | `/projects/{project_id}` | Delete project (DB + filesystem) | ✅ Done |
| GET | `/projects/{project_id}` | Get single project details | ✅ Done |
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
| PATCH | `/tags/{tag_id}` | Rename / update tag | 🟡 Planned (v0) |
| DELETE | `/tags/{tag_id}` | Delete tag | 🟡 Planned (v0) |

---

### Collections

| Method | Path | Purpose | Status |
|------|------|---------|--------|
| GET | `/collections` | List collections | 🔵 Post-v0 |
| POST | `/collections` | Create collection | 🔵 Post-v0 |
| PATCH | `/collections/{collection_id}` | Update collection | 🔵 Post-v0 |
| DELETE | `/collections/{collection_id}` | Delete collection | 🔵 Post-v0 |
| POST | `/collections/{collection_id}/projects` | Add project to collection | 🔵 Post-v0 |
| DELETE | `/collections/{collection_id}/projects/{project_id}` | Remove project from collection | 🔵 Post-v0 |

---

### Sync / Maintenance

| Method | Path | Purpose | Status |
|------|------|---------|--------|
| POST | `/sync/run` | Trigger filesystem sync (`full` only for now) | ✅ Done |
| GET | `/sync/status` | Current/last sync status | ✅ Done |
| GET | `/sync/runs` | List sync runs | ✅ Done |
| GET | `/sync/runs/{run_id}` | Get sync run details | ✅ Done |
| GET | `/sync/runs/{run_id}/events` | Sync event log | ✅ Done |
