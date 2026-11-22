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
    /// Folders removed from LiMa with their files kept (sync skips them).
    #[serde(default)]
    pub ignored_folders: Vec<String>,
    /// Collections imported from a site, with what sync knows about each model.
    #[serde(default)]
    pub collection_sources: Vec<BackupCollectionSource>,
    /// Duplicate groups marked "not duplicates".
    #[serde(default)]
    pub duplicate_dismissals: Vec<BackupDismissal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupCollectionSource {
    /// Name of the LiMa collection it fills.
    pub collection: String,
    /// `printables`, `makerworld` or `thingiverse`.
    pub source: String,
    pub source_id: String,
    pub url: String,
    #[serde(default)]
    pub keep_in_sync: bool,
    #[serde(default)]
    pub items: Vec<BackupSourceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupSourceItem {
    pub model_id: String,
    pub model_url: String,
    pub name: String,
    /// Folder of the project imported or linked for this model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_folder: Option<String>,
    /// The collection created that project (it's deleted if the model leaves the source).
    #[serde(default)]
    pub imported: bool,
    /// Left out by the user; sync never imports it.
    #[serde(default)]
    pub excluded: bool,
}

/// A hidden duplicate group: projects by folder, or a shared file by hash.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackupDismissal {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_folders: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_hash: Option<String>,
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
    /// Folders added to the ignore list.
    pub ignored_folders_added: usize,
    /// Projects removed from the index (files kept) because the backup lists their folder as ignored.
    pub projects_hidden: usize,
    /// Imported collections whose source (and sync state) came back.
    #[serde(default)]
    pub collection_sources_added: usize,
    /// Duplicate groups hidden again.
    #[serde(default)]
    pub duplicate_dismissals_added: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RenamedMatch {
    pub backup_folder: String,
    pub library_folder: String,
}
