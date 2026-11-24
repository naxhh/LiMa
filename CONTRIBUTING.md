# Contributing to LiMa

Thanks for taking a look. This file covers running, building and testing LiMa from source.
For what the project is and how to deploy it, see the [README](./README.md).

## What you need

- **Rust** stable (the workspace uses edition 2024)
- **Node.js** 22 or newer
- **pnpm** 10 or newer
- Optional: `cargo-watch` for the dev script, `sqlx-cli` for migrations, Docker for images

## Run everything

```bash
./scripts/dev.sh
```

This starts:

- the server with `cargo watch`, rebuilt on change, on <http://localhost:6767>
- the Vite UI with hot reload on <http://localhost:5173> (bound to `0.0.0.0`)

Vite proxies `/api` and `/media` to the server. Running the script again replaces the previous
instance. Open <http://localhost:5173>.

## Run the server only

```bash
cargo run -p lima-server
```

Or with rebuilds:

```bash
cargo watch -x "run -p lima-server"
```

The library defaults to `data/library` and state (database, thumbnails, uploads) to
`data/state`. Missing folders are created on start. See the [README](./README.md#configuration)
for the environment variables that change these paths.

## Run the UI only

```bash
cd ui
pnpm install
pnpm dev
```

The dev server proxies to a server on `localhost:6767`.

## Build

```bash
# server
cargo build --release -p lima-server

# UI
cd ui
pnpm build
```

The built UI is written to `ui/dist` and served by the server at `/`.

## Tests and checks

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

cd ui
pnpm exec tsc -b
pnpm lint
pnpm build
```

## API types and OpenAPI

The UI's API types are generated from the running server:

```bash
cd ui
pnpm exec openapi-typescript http://localhost:6767/api/openapi.json -o src/gen/openapi.ts
# or
pnpm gen:api
```

The OpenAPI document is also served at `/openapi.json` and browsable at `/docs`.

## Database

Migrations live in `migrations/`.

```bash
sqlx migrate add _name_
sqlx migrate run
```

Useful SQLite commands:

```bash
sqlite3 data/state/lima.db ".tables"
sqlite3 data/state/lima.db ".schema projects"
```

## Docker

```bash
docker compose up -d --build
```

The image is built in three stages: the Vite UI, the Rust server, and a small Debian runtime.
The compose file mounts `./data/library` and `./data/state`.

## Project layout

| Path | What lives there |
|------|------------------|
| `crates/server` | Axum HTTP API, routes, auth-free endpoints, static media. |
| `crates/db` | SQL queries and database models. |
| `crates/indexer` | Filesystem sync, hashing, thumbnails and preview meshes. |
| `crates/domain` | Shared models and configuration. |
| `crates/importers` | Printables, MakerWorld and Thingiverse imports. |
| `ui` | React + TypeScript single-page UI. |
| `migrations` | SQLx migrations. |
| `docs` | API, backend and frontend notes. |

## Notes

- The filesystem is the source of truth. Metadata belongs in the database, not in users' files.
- Do not change files in the library unless the feature is an upload or an explicit delete.
- Keep the API documented in `docs/API.md` and through the OpenAPI annotations.
