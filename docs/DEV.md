# LIMA: Dev notes

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