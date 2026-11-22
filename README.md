<img src="docs/brand/logo-accent.svg" alt="" width="96" align="right" />

# LiMa: Library Index for Model Assets

LiMa is a library index for 3d models and related assets.

Is not much different than MakerWorld, Thingiverse or Printables but removing the social aspect and being a self-hosted version.

The main use case is to store 3d models so you have a backup when the innevitable cloud erases it from existence for good.

We provide tags and collections for sorting while providing a strong and fast API.
You can create a new library or bring your existing file system folders to it, we will never modify your original files and metadata lives as a sidecar in the database.

## Why?

In the past I collaborated a bit in [MMP](https://github.com/Maker-Management-Platform/) to provide the features I wanted to have in a service like this one instead of condig yet another solution.

MMP seems to be now on an uncertain state and I envisioned the sync process a bit different to how MMP approaches it so I decided to take this project again since I also wanted to learn more Rust and this is a good excuse.

## Features, bugs, etc...
This is under start development and I wouldn't recommend anyone to use it yet.

Check the [docs](./docs/) folder for immediate goals and dev hints.
If you are running the project go to `/docs/` for a SwaggerUI with current API endpoints.

**UI is not a priority at the moment**


## Dev

Run:
```bash
cargo run -p lima-server
```

Environmentals:
- **LIMA_SERVER_PORT** defaults to `6767`
- **LIMA_DATA_DIR** base folder for library and state. Defaults to `data`
- **LIMA_LIBRARY_DIR** model library, one folder per project. Defaults to `$LIMA_DATA_DIR/library`
- **LIMA_STATE_DIR** app state (database, staged uploads, thumbnails). Defaults to `$LIMA_DATA_DIR/state`
- **LIMA_DATABASE_URL** defaults to `sqlite:$LIMA_STATE_DIR/lima.db?mode=rwc` (created if missing)
- **LIMA_UI_DIR** built UI served at `/`. Defaults to `ui/dist`
- **LIMA_WATCHER** watch the library folder and sync changes automatically. Defaults to `true`; set `false` to disable
- **LIMA_BACKUP_DIR** automatic metadata backups. Defaults to `$LIMA_STATE_DIR/backups`; put it on a different disk than the database
- **LIMA_BACKUP_INTERVAL_HOURS** hours between automatic backups (only written when something changed). Defaults to `24`; `0` disables
- **LIMA_BACKUP_KEEP** how many backup files to keep. Defaults to `14`
- **LIMA_TRASH_DAYS** how long deleted projects stay in the trash (`<library>/.lima-trash/`) before they're deleted for good. Defaults to `30`; `0` turns the trash off so deletes are final
- **LIMA_THINGIVERSE_TOKEN** Thingiverse app token, needed to import Thingiverse links. Create an app at https://www.thingiverse.com/apps/create (any name and URL work) and copy its **App Token**. It only reads public things. Keep it in your `env/` secrets (sops) like other credentials
- **LIMA_COLLECTION_SYNC_HOURS** how long after its last check a collection imported with "keep in sync" is checked again for added and removed models. Defaults to `12`; `0` only syncs when you press Sync. Shown on the Sync page under Sync collections

Missing library and state folders are created on start. For containers, mount the library
(read/write: uploads and deletes write to it) and the state folder as volumes, e.g.
`LIMA_LIBRARY_DIR=/library LIMA_STATE_DIR=/state`.

### Metadata backup and restore

Files on disk are the source of truth; names, descriptions, covers, tags, collections and
favourites live only in the database. LiMa writes a JSON backup of them to `LIMA_BACKUP_DIR`
every day, and the **Sync → Backups** page lets you back up now, download an export, and
restore. To recover after losing the database: start LiMa, run a sync so the projects exist
again, then restore the latest backup. Projects are matched by folder; a folder renamed since
the backup is matched by its files' hashes.
