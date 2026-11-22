//! Content hashing used for change detection.

use std::fs::File;
use std::io;
use std::path::Path;

/// Blake3 hex digest of a file's contents. Blocking: call through `spawn_blocking`.
pub fn hash_file(path: &Path) -> io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(File::open(path)?)?;
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn hashes_contents() {
        let path = std::env::temp_dir().join(format!("lima-hash-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"hello").unwrap();
        let hash = super::hash_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(hash, blake3::hash(b"hello").to_hex().to_string());
    }
}
