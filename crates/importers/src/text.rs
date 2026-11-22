/// Converts a site's HTML description to Markdown, which is how LiMa stores and
/// renders descriptions (headings, bold/italic, lists, inline links, images).
pub fn html_to_markdown(html: &str) -> String {
    if html.trim().is_empty() {
        return String::new();
    }
    let converter = htmd::HtmlToMarkdown::builder().skip_tags(vec!["script", "style", "iframe", "noscript"]).build();
    let md = converter.convert(html).unwrap_or_else(|_| html.to_string());

    // Collapse runs of blank lines (sites pad with empty paragraphs) and trailing spaces,
    // keeping Markdown's two-space hard breaks.
    let mut out = String::with_capacity(md.len());
    let mut blank = 0;
    for line in md.lines() {
        let line = if line.ends_with("  ") && !line.trim().is_empty() { line } else { line.trim_end() };
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
            out.push('\n');
            continue;
        }
        blank = 0;
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_html_to_markdown() {
        let md = html_to_markdown(
            "<h3>Update</h3><p>Hello <b>world</b> &amp; <a href=\"https://x.y\">link</a></p><p><br><br></p><p><br></p><ul><li>one</li><li>two</li></ul><script>alert(1)</script>",
        );
        assert!(md.contains("### Update"), "{md}");
        assert!(md.contains("Hello **world**"), "{md}");
        assert!(md.contains("[link](https://x.y)"), "{md}");
        assert!(md.lines().any(|l| l.starts_with('*') && l.ends_with("one")), "{md}");
        assert!(!md.contains("alert"), "{md}");
        assert!(!md.contains("\n\n\n"), "{md}");
        assert_eq!(html_to_markdown("   "), "");
    }
}
