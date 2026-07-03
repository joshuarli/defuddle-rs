/// Check if a URL uses a dangerous protocol (javascript:, data:text/html).
/// Strips whitespace and control characters before checking.
pub fn is_dangerous_url(url: &str) -> bool {
    let normalized: String = url
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect();
    let lower = normalized.to_lowercase();
    lower.starts_with("javascript:") || lower.starts_with("data:text/html")
}

/// Remove dangerous elements and attributes from the document.
pub fn strip_unsafe_elements(doc: &mut crate::dom::Document) {
    let dangerous_selectors = [
        "script",
        "style",
        "noscript",
        "object",
        "embed",
        "applet",
        "base",
    ];

    for selector in &dangerous_selectors {
        let matches = doc.query_selector_all(selector);
        for id in matches {
            doc.remove(id);
        }
    }

    let all_elements = doc.query_selector_all("*");
    for id in all_elements {
        let tag = doc.tag_name(id);
        if tag.is_empty() {
            continue;
        }

        let attrs_to_check = ["href", "src", "action", "formaction", "xlink:href"];
        for attr_name in &attrs_to_check {
            if let Some(value) = doc.get_attr(id, attr_name) {
                if is_dangerous_url(value) {
                    doc.remove_attr(id, attr_name);
                }
            }
        }

        if doc.get_attr(id, "srcdoc").is_some() {
            doc.remove_attr(id, "srcdoc");
        }

        let event_handlers = [
            "onclick", "ondblclick", "onmousedown", "onmouseup", "onmouseover",
            "onmousemove", "onmouseout", "onkeypress", "onkeydown", "onkeyup",
            "onload", "onerror", "onfocus", "onblur", "onsubmit", "onreset",
            "onchange", "onselect", "oninput", "onabort", "oncanplay",
            "oncanplaythrough", "ondurationchange", "onemptied", "onended",
            "onloadeddata", "onloadedmetadata", "onloadstart", "onpause",
            "onplay", "onplaying", "onprogress", "onratechange", "onseeked",
            "onseeking", "onstalled", "onsuspend", "ontimeupdate",
            "onvolumechange", "onwaiting", "onwheel", "onscroll", "onresize",
            "ontoggle", "oncopy", "oncut", "onpaste", "ondrag", "ondragend",
            "ondragenter", "ondragleave", "ondragover", "ondragstart",
            "ondrop", "oncontextmenu",
        ];

        for event in &event_handlers {
            if doc.get_attr(id, event).is_some() {
                doc.remove_attr(id, event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::dom::Document;
    use super::*;

    #[test]
    fn test_is_dangerous_url() {
        assert!(is_dangerous_url("javascript:alert(1)"));
        assert!(is_dangerous_url("  Javascript:void(0)"));
        assert!(is_dangerous_url("data:text/html,<script>alert(1)</script>"));
        assert!(!is_dangerous_url("https://example.com"));
        assert!(!is_dangerous_url("/relative/path"));
        assert!(!is_dangerous_url("mailto:test@example.com"));
    }

    #[test]
    fn test_strip_dangerous_elements() {
        let mut doc = Document::parse(
            r#"
            <!DOCTYPE html>
            <html>
            <body>
                <script>alert('xss')</script>
                <style>body { color: red; }</style>
                <noscript>No JS</noscript>
                <object data="test.swf"></object>
                <embed src="test.swf">
                <applet code="Test.class"></applet>
                <base href="https://evil.com">
                <p>Safe content</p>
            </body>
            </html>
            "#,
        );

        strip_unsafe_elements(&mut doc);

        let html = doc.outer_html(doc.root());
        assert!(!html.contains("<script"), "script should be removed");
        assert!(!html.contains("<style"), "style should be removed");
        assert!(!html.contains("<noscript"), "noscript should be removed");
        assert!(!html.contains("<object"), "object should be removed");
        assert!(!html.contains("<embed"), "embed should be removed");
        assert!(!html.contains("<applet"), "applet should be removed");
        assert!(!html.contains("<base"), "base should be removed");
        assert!(html.contains("Safe content"), "safe content should remain");
    }

    #[test]
    fn test_strip_dangerous_attributes() {
        let mut doc = Document::parse(
            r#"
            <!DOCTYPE html>
            <html>
            <body>
                <a href="javascript:alert(1)">Click me</a>
                <a href="https://safe.com">Safe link</a>
                <img src="data:text/html,<script>alert(1)</script>">
                <iframe srcdoc="<script>alert(1)</script>"></iframe>
                <div onclick="doBad()">Click</div>
            </body>
            </html>
            "#,
        );

        strip_unsafe_elements(&mut doc);

        let html = doc.outer_html(doc.root());
        assert!(!html.contains("javascript:"), "javascript: URLs should be removed");
        assert!(!html.contains("data:text/html"), "data:text/html URLs should be removed");
        assert!(!html.contains("srcdoc"), "srcdoc should be removed");
        assert!(!html.contains("onclick"), "onclick should be removed");
        assert!(html.contains("https://safe.com"), "safe URLs should remain");
        assert!(html.contains("Safe link"), "safe link text should remain");
    }
}
