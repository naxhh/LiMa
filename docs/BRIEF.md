# Project Goal Brief

## Project Name
LIMA: Library Index for Model Assets

## Purpose
This project is a self-hosted backend service for managing collections of 3D models and related assets (images, documents, etc.) stored directly on the filesystem.

The filesystem is the **source of truth** for binary content, while a SQL database is used as an indexed metadata layer to enable fast search, tagging, collections, and operational visibility.

The project is intentionally **API-first**, with no user authentication or authorization concerns. A UI may be added later, but is explicitly out of scope for the initial phase.
We may introduce tokens for the API but not in first iterations.

## Core Goals
- Provide a robust, well-structured HTTP API for managing 3D model libraries
- Store projects as plain folders with human-readable names
- Maintain a synchronized metadata database for fast querying
- Support both explicit ingestion (API uploads) and implicit ingestion (filesystem folders)
- Enable deterministic and observable background synchronization with Immich as inspiration
- Serve as a learning vehicle for idiomatic, production-quality Rust

## Non-Goals (Initial Phase)
- User accounts, authentication, or permissions
- Distributed or multi-node operation
- Real-time collaboration
- Complex asset processing (e.g., slicing, rendering, CAD inspection)
- Printers integrations
- Stock management


## High-Level Concepts

### Project
A project corresponds to a directory inside the library root.  
The directory name is the default project name.

### Asset
An asset is a file contained within a project directory. Assets are categorized
by type (model, image, other).

Assets are **NOT** altered by this software in any way.

### Metadata
Metadata (tags, collections, main image, notes, etc) is stored in a SQL
database and is not embedded into files or folders.

### Synchronization
A background indexing system keeps the database in sync with the filesystem via:
- On-demand full scans
- Incremental scans
- Filesystem event watching (best-effort)

The system is designed so the database can always be rebuilt from disk without metadata.
Metadata can be backed up and restored into a new database.

## Architecture Summary
- Single Rust binary
- Axum-based HTTP API
- SQLite (default) with FTS for search
- Tokio async runtime
- Background worker for indexing and sync
- Filesystem-backed storage for all binary content

## Deliverables (Phase 1)

### Functional Deliverables
- [HTTP API](./API.md) for:
  - Projects
  - Assets
  - Tags
  - Collections
  - Search
  - Sync/index runs
- Hybrid synchronization system (watcher + manual scan)
- Deterministic asset hashing for change detection
- Thumbnail generation for image assets
- Operational endpoints for sync status and diagnostics

### Technical Deliverables
- Rust workspace with modular crates
- SQL migrations and schema
- Structured logging and error handling
- Clear separation between API, domain logic, and indexing logic
- OpenAPI-style API documentation (static)

### Quality Deliverables
- Idempotent API behavior where applicable
- Explicit error responses with machine-readable codes
- Deterministic sync runs with recorded history
- Rebuildable DB from filesystem alone (with assumed metadata loss)

## Success Criteria
- A library directory (either empty or not) can be indexed into a usable database
- Dropping files or folders into the library is reflected after a sync
- Metadata operations do not modify filesystem layout (exception being uploads)
- Sync runs are observable, repeatable, and debuggable
- The API is stable enough to build a UI on later without redesign

## Deliverables (Phase 2)

To be defined
