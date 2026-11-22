/// Converts a site's HTML description to the plain text Lima shows
/// (headings as `#`, lists as `*`, links as `[text][n]` with the URLs listed at the end).
pub fn html_to_text(html: &str) -> String {
    if html.trim().is_empty() {
        return String::new();
    }
    // Wide enough that paragraphs are never re-wrapped.
    let text = html2text::config::plain()
        .string_from_read(html.as_bytes(), 100_000)
        .unwrap_or_else(|_| html.to_string());

    // Collapse runs of blank lines and trailing spaces.
    let mut out = String::with_capacity(text.len());
    let mut blank = 0;
    for line in text.lines().map(str::trim_end) {
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_html() {
        let text = html_to_text("<h3>Update</h3><p>Hello <b>world</b> &amp; <a href=\"https://x.y\">link</a></p><p><br><br></p><ul><li>one</li><li>two</li></ul>");
        assert!(text.contains("Update"), "{text}");
        assert!(text.contains("Hello world & [link]"), "{text}");
        assert!(text.contains("https://x.y"), "{text}");
        assert!(text.contains("* one"), "{text}");
        assert!(!text.contains("\n\n\n"), "{text}");
        assert_eq!(html_to_text("   "), "");
    }
}
