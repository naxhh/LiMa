//! The library search language: free words plus filters.
//!
//! | Filter | Matches |
//! |---|---|
//! | `#pla`, `tag:pla` | projects tagged PLA |
//! | `in:desk`, `collection:desk` | projects in the Desk collection (`in:favourites` too) |
//! | `is:fav` | favourites |
//! | `format:stl`, `ext:3mf` | projects with a model of that format |
//! | `file:benchy` | projects with a file whose path contains "benchy" |
//! | `has:image`, `no:image` | projects with / without pictures |
//! | `from:printables` | projects imported from Printables (or MakerWorld, Thingiverse, a domain) |
//! | `by:prusa`, `author:prusa` | imported projects whose author contains "prusa" |
//!
//! A leading `-` negates (`-#pla`, `-in:desk`, `-is:fav`, `-has:image`). Quotes keep spaces:
//! `#"silk pla"`, `in:"living room"`. Anything else is searched as words.

use crate::queries::projects::ProjectFilter;

#[derive(Debug, Default, PartialEq)]
pub struct ParsedSearch {
    /// Words left for the full-text search.
    pub text: String,
}

/// Splits on spaces, keeping `"quoted parts"` (also after `key:` or `#`) together. Quotes are removed.
fn tokens(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in input.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn source_host(value: &str) -> String {
    match value {
        "printables" => "printables.com".into(),
        "makerworld" | "bambu" => "makerworld.com".into(),
        "thingiverse" => "thingiverse.com".into(),
        other => other.into(),
    }
}

/// Moves the filters in `input` into `filter` and returns the remaining words.
pub fn parse_into(input: &str, filter: &mut ProjectFilter) -> ParsedSearch {
    let mut words = Vec::new();
    for token in tokens(input) {
        let (negated, body) = match token.strip_prefix('-') {
            Some(rest) if rest.len() > 1 && (rest.starts_with('#') || rest.contains(':')) => (true, rest),
            _ => (false, token.as_str()),
        };
        let (key, value) = if let Some(tag) = body.strip_prefix('#') {
            ("tag", tag)
        } else if let Some((k, v)) = body.split_once(':') {
            (k, v)
        } else {
            words.push(token.clone());
            continue;
        };
        let value = value.trim().to_lowercase();
        if value.is_empty() {
            words.push(token.clone());
            continue;
        }
        match (key.to_ascii_lowercase().as_str(), negated) {
            ("tag" | "tags", false) => filter.tag_names.push(value),
            ("tag" | "tags", true) => filter.not_tag_names.push(value),
            ("in" | "collection" | "col", false) => filter.collections.push(value),
            ("in" | "collection" | "col", true) => filter.not_collections.push(value),
            ("is", n) if matches!(value.as_str(), "fav" | "favs" | "favourite" | "favorite" | "favourites" | "favorites" | "starred") => {
                filter.favourite = Some(!n)
            }
            ("format" | "ext" | "type", false) => {
                let f = value.trim_start_matches('.').to_ascii_uppercase();
                if !f.is_empty() && f.len() <= 10 && f.chars().all(|c| c.is_ascii_alphanumeric()) && !filter.formats.contains(&f) {
                    filter.formats.push(f);
                }
            }
            ("has", n) if matches!(value.as_str(), "image" | "images" | "picture" | "pictures" | "photo" | "photos" | "cover") => {
                filter.has_image = Some(!n)
            }
            ("no", false) if matches!(value.as_str(), "image" | "images" | "picture" | "pictures" | "photo" | "photos" | "cover") => {
                filter.has_image = Some(false)
            }
            ("from" | "source", false) => filter.sources.push(source_host(&value)),
            ("by" | "author", false) => filter.authors.push(value),
            ("file" | "filename" | "files", false) => filter.files.push(value),
            // Not a filter (or a negation we don't support): search it as words.
            _ => words.push(token.clone()),
        }
    }
    ParsedSearch { text: words.join(" ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(q: &str) -> (String, ProjectFilter) {
        let mut f = ProjectFilter::default();
        let p = parse_into(q, &mut f);
        (p.text, f)
    }

    #[test]
    fn reads_filters_and_leaves_words() {
        let (text, f) = parse(r##"dragon #PLA -#gift in:"Living Room" -in:garage is:fav format:.stl file:Benchy from:printables by:Prusa has:image"##);
        assert_eq!(text, "dragon");
        assert_eq!(f.tag_names, vec!["pla"]);
        assert_eq!(f.not_tag_names, vec!["gift"]);
        assert_eq!(f.collections, vec!["living room"]);
        assert_eq!(f.not_collections, vec!["garage"]);
        assert_eq!(f.favourite, Some(true));
        assert_eq!(f.formats, vec!["STL"]);
        assert_eq!(f.files, vec!["benchy"]);
        assert_eq!(f.sources, vec!["printables.com"]);
        assert_eq!(f.authors, vec!["prusa"]);
        assert_eq!(f.has_image, Some(true));
    }

    #[test]
    fn negations_and_plain_words() {
        let (text, f) = parse(r##"-is:fav no:image #"silk pla" print-in-place -dash https://x.y/z wat:ever #"##);
        assert_eq!(f.favourite, Some(false));
        assert_eq!(f.has_image, Some(false));
        assert_eq!(f.tag_names, vec!["silk pla"]);
        assert_eq!(text, "print-in-place -dash https://x.y/z wat:ever #", "unknown keys and dashes stay words");
    }
}
