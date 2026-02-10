# LIMA: Dev notes

## Server

```bash
cargo watch -x "run -p lima-server"
```


## Frontend

```bash
pnpm dev
```

Update openapi definitions
```bash
pnpm exec openapi-typescript http://localhost:6767/openapi.json -o src/gen/openapi.ts

pnpm gen:api
```

## DB

- Add migration

```bash
sqlx migrate add _name_
```

- Run migrations

```bash
sqlx migrate run
```

- Query SQL

```bash
sqlite3 data/state/lima.db ".tables"
sqlite3 data/state/lima.db "SELECT name, type FROM sqlite_master WHERE name LIKE 'projects_%';"
sqlite3 data/state/lima.db ".schema projects"
```