/// Longest tag name accepted, in characters.
pub const TAG_NAME_MAX_CHARS: usize = 64;

/// Trims and collapses inner whitespace. `None` when empty or too long.
pub fn normalize_tag_name(raw: &str) -> Option<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty() && name.chars().count() <= TAG_NAME_MAX_CHARS).then_some(name)
}

/// Normalizes a `#RRGGBB` color to upper case. `None` when malformed.
pub fn normalize_tag_color(raw: &str) -> Option<String> {
    let c = raw.trim();
    let hex = c.strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit())).then(|| format!("#{}", hex.to_ascii_uppercase()))
}

/// Deterministic color derived from the name, used when none is given.
pub fn default_tag_color(name: &str) -> String {
    let mut hash: u32 = 0;
    for byte in name.bytes() {
        hash = (byte as u32).wrapping_add(hash.wrapping_mul(31));
    }
    format!("#{:02X}{:02X}{:02X}", (hash >> 16) & 0xFF, (hash >> 8) & 0xFF, hash & 0xFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(normalize_tag_name("  pla   plus "), Some("pla plus".into()));
        assert_eq!(normalize_tag_name("   "), None);
        assert_eq!(normalize_tag_name(&"x".repeat(65)), None);
        assert_eq!(normalize_tag_name(&"ñ".repeat(64)), Some("ñ".repeat(64)));
    }

    #[test]
    fn colors() {
        assert_eq!(normalize_tag_color("#a1b2c3"), Some("#A1B2C3".into()));
        assert_eq!(normalize_tag_color(" #A1B2C3 "), Some("#A1B2C3".into()));
        for bad in ["a1b2c3", "#a1b2c", "#a1b2c3d", "#ggggggg", "#gggggg", ""] {
            assert_eq!(normalize_tag_color(bad), None, "{bad}");
        }
    }

    #[test]
    fn default_color_is_stable_and_valid() {
        assert_eq!(default_tag_color("miniatures"), default_tag_color("miniatures"));
        assert!(normalize_tag_color(&default_tag_color("miniatures")).is_some());
    }
}
