/// Longest collection name accepted, in characters.
pub const COLLECTION_NAME_MAX_CHARS: usize = 100;
/// Longest collection description accepted, in characters.
pub const COLLECTION_DESCRIPTION_MAX_CHARS: usize = 2000;

/// Trims and collapses inner whitespace. `None` when empty or too long.
pub fn normalize_collection_name(raw: &str) -> Option<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty() && name.chars().count() <= COLLECTION_NAME_MAX_CHARS).then_some(name)
}

/// Trims surrounding whitespace (inner newlines are kept). `None` when too long.
pub fn normalize_collection_description(raw: &str) -> Option<String> {
    let d = raw.trim();
    (d.chars().count() <= COLLECTION_DESCRIPTION_MAX_CHARS).then(|| d.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_descriptions() {
        assert_eq!(normalize_collection_name("  Desk   toys "), Some("Desk toys".into()));
        assert_eq!(normalize_collection_name(" "), None);
        assert_eq!(normalize_collection_name(&"x".repeat(101)), None);
        assert_eq!(normalize_collection_description("  line 1\nline 2 \n"), Some("line 1\nline 2".into()));
        assert_eq!(normalize_collection_description(&"x".repeat(2001)), None);
    }
}
