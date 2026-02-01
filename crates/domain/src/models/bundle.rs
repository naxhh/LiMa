use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BundleMeta {
    pub uploaded_at: String,
    pub files: Vec<FileMeta>
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileMeta {
    pub name: String,
    pub size: i64,
    pub mtime: Option<String>,
    pub mime: String,
    pub kind: String,
    pub checksum: Option<String>,
}
