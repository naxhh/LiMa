//! Filesystem locations used by LIMA.
//!
//! | Variable            | Default                          |
//! |---------------------|----------------------------------|
//! | `LIMA_DATA_DIR`     | `data`                           |
//! | `LIMA_LIBRARY_DIR`  | `$LIMA_DATA_DIR/library`         |
//! | `LIMA_STATE_DIR`    | `$LIMA_DATA_DIR/state`           |
//! | `LIMA_DATABASE_URL` | `sqlite:$LIMA_STATE_DIR/lima.db` |
//! | `LIMA_UI_DIR`       | `ui/dist`                        |
//! | `LIMA_BACKUP_DIR`   | `$LIMA_STATE_DIR/backups`        |

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// Model library: one folder per project. Source of truth for files.
    pub library: PathBuf,
    /// App state: database (by default), staged bundles, thumbnails.
    pub state: PathBuf,
    /// Built SPA served at `/`.
    pub ui: PathBuf,
    pub database_url: String,
    /// Automatic metadata backups. Point it at a different disk than the database
    /// so losing one doesn't lose the other.
    pub backups: PathBuf,
}

impl Paths {
    /// Reads the configuration from the process environment.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Builds the configuration from any key lookup (testable without touching the env).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let var = |key: &str| get(key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());

        let data = PathBuf::from(var("LIMA_DATA_DIR").unwrap_or_else(|| "data".into()));
        let library = var("LIMA_LIBRARY_DIR").map(PathBuf::from).unwrap_or_else(|| data.join("library"));
        let state = var("LIMA_STATE_DIR").map(PathBuf::from).unwrap_or_else(|| data.join("state"));
        let ui = var("LIMA_UI_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("ui/dist"));
        let database_url = var("LIMA_DATABASE_URL")
            .unwrap_or_else(|| format!("sqlite:{}?mode=rwc", state.join("lima.db").display()));

        let backups = var("LIMA_BACKUP_DIR").map(PathBuf::from).unwrap_or_else(|| state.join("backups"));

        Self { library, state, ui, database_url, backups }
    }

    /// Temporary upload staging area.
    /// A stored backup file. `None` unless `name` is a plain `metadata-*.json` file name.
    pub fn backup_file(&self, name: &str) -> Option<PathBuf> {
        let valid = name.starts_with("metadata-")
            && name.ends_with(".json")
            && name.len() <= 100
            && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && !name.contains("..");
        valid.then(|| self.backups.join(name))
    }

    pub fn bundles(&self) -> PathBuf {
        self.state.join("bundles")
    }

    /// Folder of a staged bundle. `None` unless `bundle_id` looks like a UUID,
    /// so ids coming from URLs can never escape the bundles folder (`..`, `/`).
    pub fn bundle(&self, bundle_id: &str) -> Option<PathBuf> {
        let valid = bundle_id.len() == 36 && bundle_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        valid.then(|| self.bundles().join(bundle_id))
    }

    pub fn thumbnails(&self) -> PathBuf {
        self.state.join("thumbnails")
    }

    pub fn project_dir(&self, folder_path: &str) -> PathBuf {
        self.library.join(folder_path)
    }

    /// Directories that must exist before the server starts.
    pub fn required_dirs(&self) -> [&Path; 3] {
        [&self.library, &self.state, &self.backups]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn paths(vars: &[(&str, &str)]) -> Paths {
        let map: HashMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Paths::from_lookup(|k| map.get(k).cloned())
    }

    #[test]
    fn defaults_match_previous_layout() {
        let p = paths(&[]);
        assert_eq!(p.library, PathBuf::from("data/library"));
        assert_eq!(p.state, PathBuf::from("data/state"));
        assert_eq!(p.ui, PathBuf::from("ui/dist"));
        assert_eq!(p.database_url, "sqlite:data/state/lima.db?mode=rwc");
        let id = "0b5e6a3c-1f2d-4c3b-9a8e-7d6c5b4a3f21";
        assert_eq!(p.bundle(id), Some(PathBuf::from(format!("data/state/bundles/{id}"))));
        assert_eq!(p.thumbnails(), PathBuf::from("data/state/thumbnails"));
        assert_eq!(p.backups, PathBuf::from("data/state/backups"));
    }

    #[test]
    fn backup_file_names_are_validated() {
        let p = paths(&[("LIMA_BACKUP_DIR", "/backups")]);
        assert_eq!(p.backup_file("metadata-2026-11-22T10-00-00Z.json"), Some(PathBuf::from("/backups/metadata-2026-11-22T10-00-00Z.json")));
        for bad in ["../metadata-x.json", "metadata-x.json/..", "metadata-..json", "other.json", "metadata-x.txt", "metadata-a/b.json"] {
            assert_eq!(p.backup_file(bad), None, "{bad}");
        }
    }

    #[test]
    fn rejects_bundle_ids_that_escape() {
        let p = paths(&[]);
        for bad in ["..", "../state", "a/b", "", "0b5e6a3c-1f2d-4c3b-9a8e-7d6c5b4a3f2/", "..................................../"] {
            assert_eq!(p.bundle(bad), None, "{bad}");
        }
    }

    #[test]
    fn data_dir_moves_everything() {
        let p = paths(&[("LIMA_DATA_DIR", "/srv/lima")]);
        assert_eq!(p.library, PathBuf::from("/srv/lima/library"));
        assert_eq!(p.database_url, "sqlite:/srv/lima/state/lima.db?mode=rwc");
    }

    #[test]
    fn specific_dirs_override_data_dir() {
        let p = paths(&[
            ("LIMA_DATA_DIR", "/srv/lima"),
            ("LIMA_LIBRARY_DIR", "/mnt/models"),
            ("LIMA_DATABASE_URL", "sqlite:/db/lima.db"),
            ("LIMA_UI_DIR", " "), // blank means default
        ]);
        assert_eq!(p.library, PathBuf::from("/mnt/models"));
        assert_eq!(p.state, PathBuf::from("/srv/lima/state"));
        assert_eq!(p.database_url, "sqlite:/db/lima.db");
        assert_eq!(p.ui, PathBuf::from("ui/dist"));
    }
}
