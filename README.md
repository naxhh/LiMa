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

Missing library and state folders are created on start. For containers, mount the library
(read/write: uploads and deletes write to it) and the state folder as volumes, e.g.
`LIMA_LIBRARY_DIR=/library LIMA_STATE_DIR=/state`.
