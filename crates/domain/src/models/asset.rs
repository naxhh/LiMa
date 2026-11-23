//! The kinds of files LiMa tells apart, by extension.

/// What a file is, as stored in `assets.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    /// Printable geometry: shown with model badges, previews and slicer buttons.
    Model,
    /// Pictures: thumbnails, covers.
    Image,
    /// Everything else (zips, PDFs, notes...).
    Other,
}

impl AssetKind {
    /// Model formats (lower case).
    pub const MODEL_EXTENSIONS: &'static [&'static str] = &["stl", "obj", "3mf", "step", "stp", "fbx", "glb", "gltf"];
    /// Image formats (lower case).
    pub const IMAGE_EXTENSIONS: &'static [&'static str] = &["png", "jpg", "jpeg", "webp"];

    /// Classifies a file name or path by its extension, ignoring case.
    pub fn from_path(path: &str) -> Self {
        let lower = path.to_ascii_lowercase();
        let ext = lower.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
        if Self::MODEL_EXTENSIONS.contains(&ext) {
            AssetKind::Model
        } else if Self::IMAGE_EXTENSIONS.contains(&ext) {
            AssetKind::Image
        } else {
            AssetKind::Other
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AssetKind::Model => "model",
            AssetKind::Image => "image",
            AssetKind::Other => "other",
        }
    }
}

/// `AssetKind::from_path(path).as_str()`: the value stored in the database.
pub fn asset_kind(path: &str) -> &'static str {
    AssetKind::from_path(path).as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_extension() {
        assert_eq!(AssetKind::from_path("Benchy.STL"), AssetKind::Model);
        assert_eq!(AssetKind::from_path("parts/bracket.step"), AssetKind::Model);
        assert_eq!(AssetKind::from_path("cover.jpeg"), AssetKind::Image);
        assert_eq!(AssetKind::from_path("README"), AssetKind::Other);
        assert_eq!(AssetKind::from_path("notes.pdf"), AssetKind::Other);
        assert_eq!(asset_kind("a.3mf"), "model");
    }
}
