//! Metadata backup format: everything the user added on top of the files on disk.
//!
//! Projects are keyed by `folder_path` (stable across database rebuilds). Each
//! project also lists the hashes of its files so a restore can find a project
//! whose folder was renamed. Asset ids are never stored: they change when the
//! database is rebuilt, so the cover image is referenced by file path (and hash).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const METADATA_FORMAT: &str = "lima-metadata";
pub const METADATA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetadataBackup {
    /// Always `lima-metadata`.
    pub format: String,
    /// Format version; readers reject versions newer than they know.
    pub version: u32,
    pub exported_at: String,
    #[serde(default)]
    pub tags: Vec<BackupTag>,
    #[serde(default)]
    pub collections: Vec<BackupCollection>,
    #[serde(default)]
    pub projects: Vec<BackupProject>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupTag {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupCollection {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// `favourites` for the built-in Favourites collection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_key: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupProject {
    pub folder_path: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Cover image as a path relative to the project folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_image: Option<String>,
    /// Hash of the cover image, to find it again if it was renamed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_image_hash: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Collection names (the built-in Favourites is listed by its key, see `BackupCollection`).
    #[serde(default)]
    pub collections: Vec<BackupMembership>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<BackupSource>,
    /// blake3 hashes of the project's files, sorted. Used to match renamed folders.
    #[serde(default)]
    pub file_hashes: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupMembership {
    /// Collection name, or `None` when `system_key` identifies it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_key: Option<String>,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupSource {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

/// What a restore did (or would do, for a dry run).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct RestoreReport {
    pub dry_run: bool,
    pub projects_in_backup: usize,
    /// Restored onto the project with the same folder.
    pub matched_by_folder: usize,
    /// Restored onto a project whose folder was renamed, found by its file hashes.
    pub matched_by_files: Vec<RenamedMatch>,
    /// Folders in the backup that aren't in the library (nothing to attach to).
    pub unmatched: Vec<String>,
    pub projects_updated: usize,
    pub tags_created: usize,
    pub tag_links_added: usize,
    pub collections_created: usize,
    pub collection_links_added: usize,
    pub covers_set: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RenamedMatch {
    pub backup_folder: String,
    pub library_folder: String,
}
