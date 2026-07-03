use crate::dom::{Document, NodeId};

/// Convert HTML content to Markdown.
/// Uses html-to-markdown-rs for the base conversion and adds Defuddle-specific formatting.
pub fn html_to_markdown(html: &str) -> String {
    use html_to_markdown_rs::convert;

    let cleaned = preprocess_for_markdown(html);

    let options = html_to_markdown_rs::ConversionOptions::builder()
        .heading_style(html_to_markdown_rs::HeadingStyle::Atx)
        .link_style(html_to_markdown_rs::LinkStyle::Inline)
        .autolinks(true)
        .build();

    match convert(&cleaned, options) {
        Ok(result) => {
            let md = result.content.unwrap_or_default();
            postprocess_markdown(&md)
        }
        Err(_) => cleaned,
    }
}

/// Preprocess HTML to improve markdown conversion quality
fn preprocess_for_markdown(html: &str) -> String {
    html.replace("&nbsp;", " ")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
}

/// Postprocess markdown to clean up artifacts
fn postprocess_markdown(md: &str) -> String {
    let mut result = String::new();
    let mut prev_blank = false;

    for line in md.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if !prev_blank {
                result.push('\n');
                prev_blank = true;
            }
        } else {
            result.push_str(trimmed);
            result.push('\n');
            prev_blank = false;
        }
    }

    result.trim_end().to_string()
}

/// Convert the content of a DOM element to Markdown.
pub fn content_to_markdown(doc: &Document, content_id: NodeId) -> String {
    let html = doc.inner_html(content_id);
    html_to_markdown(&html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_markdown_conversion() {
        let html = "<h1>Title</h1><p>Hello <strong>world</strong></p>";
        let md = html_to_markdown(html);
        assert!(md.contains("# Title"));
        assert!(md.contains("**world**"));
    }

    #[test]
    fn test_markdown_links() {
        let html = r#"<p><a href="https://example.com">Example</a></p>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("[Example](https://example.com)"));
    }

    #[test]
    fn test_markdown_lists() {
        let html = "<ul><li>Item 1</li><li>Item 2</li></ul>";
        let md = html_to_markdown(html);
        assert!(md.contains("- Item 1"));
        assert!(md.contains("- Item 2"));
    }
}
