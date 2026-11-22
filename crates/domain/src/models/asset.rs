/// Classify a file by its extension into one of the asset kinds stored in the DB
/// (`model`, `image`, `other`).
pub fn asset_kind(filename: &str) -> &'static str {
    let lower = filename.to_ascii_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");

    match ext {
        "png" | "jpg" | "jpeg" | "webp" => "image",
        "stl" | "obj" | "3mf" | "fbx" | "glb" | "gltf" => "model",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::asset_kind;

    #[test]
    fn classifies_by_extension() {
        assert_eq!(asset_kind("Benchy.STL"), "model");
        assert_eq!(asset_kind("cover.jpeg"), "image");
        assert_eq!(asset_kind("README"), "other");
        assert_eq!(asset_kind("notes.pdf"), "other");
    }
}
