/// Check if a URL uses a dangerous protocol.
/// Strips whitespace and control characters before checking.
pub fn is_dangerous_url(url: &str) -> bool {
    is_dangerous_url_with_inline_image(url, true)
}

pub(crate) fn is_dangerous_url_with_inline_image(url: &str, allow_inline_image: bool) -> bool {
    let normalized: String = url
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect();
    let lower = normalized.to_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("blob:") {
        return true;
    }
    lower.starts_with("data:") && !(allow_inline_image && lower.starts_with("data:image/"))
}

/// Sanitize a raw HTML fragment that did not come from the parsed page DOM
/// (e.g. schema.org text used as a content fallback), returning safe inner HTML.
pub fn sanitize_html_fragment(html: &str) -> String {
    if html.is_empty() {
        return String::new();
    }
    let mut doc = crate::dom::Document::parse(html);
    strip_unsafe_elements(&mut doc);
    let body = doc
        .query_selector_all("body")
        .into_iter()
        .next()
        .unwrap_or_else(|| doc.root());
    doc.inner_html(body)
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
        "frame",
        "frameset",
        // A browser's template.content is an inert DocumentFragment that
        // querySelectorAll cannot see, letting dangerous descendants evade
        // upstream's traversal (GHSA-3cr7-rgx8-76c3). This port's DOM has no
        // such fragment: template children are ordinary tree nodes already
        // reached by the selectors above. Removing the wrapper itself still
        // matches upstream's output and avoids leaking inert template markup.
        // Declarative shadow root templates are unwrapped by
        // shadow_dom::flatten_declarative_shadow_roots before this runs, so
        // only non-shadow-root templates reach this selector.
        "template",
        // SVG SMIL animation elements can mutate sanitized URL attributes
        // (e.g. animate an href back to a javascript: URL) after sanitization runs.
        "animate",
        "set",
        "animatemotion",
        "animatetransform",
        "animatecolor",
        "discard",
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
            if let Some(value) = doc.get_attr(id, attr_name)
                && is_dangerous_url_with_inline_image(
                    value,
                    !(*attr_name == "src" && doc.tag_name_is(id, "iframe")),
                )
            {
                doc.remove_attr(id, attr_name);
            }
        }

        if doc.get_attr(id, "srcdoc").is_some() {
            doc.remove_attr(id, "srcdoc");
        }

        doc.remove_event_handler_attrs(id);
    }
}

pub fn remove_iframes(doc: &mut crate::dom::Document) {
    for id in doc.query_selector_all("iframe") {
        doc.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::Document;

    #[test]
    fn test_is_dangerous_url() {
        assert!(is_dangerous_url("javascript:alert(1)"));
        assert!(is_dangerous_url("  Javascript:void(0)"));
        assert!(is_dangerous_url("blob:https://example.com/id"));
        assert!(is_dangerous_url("data:text/html,<script>alert(1)</script>"));
        assert!(!is_dangerous_url("data:image/png;base64,AAAA"));
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
                <frame src="https://evil.com/frame">
                <frameset><frame src="https://evil.com/nested"></frameset>
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
        assert!(!html.contains("<frame"), "frame should be removed");
        assert!(!html.contains("<frameset"), "frameset should be removed");
        assert!(html.contains("Safe content"), "safe content should remain");
    }

    #[test]
    fn test_strip_template_elements() {
        // GHSA-3cr7-rgx8-76c3: template payloads, including ones nested in
        // foreign (SVG) content, must not survive sanitization.
        let mut doc = Document::parse(
            r#"
            <!DOCTYPE html>
            <html>
            <body>
                <template><img src="x" onerror="alert(1)"><script>alert(2)</script></template>
                <svg><template><img src="y" onerror="alert(3)"></template></svg>
                <p>Safe content</p>
            </body>
            </html>
            "#,
        );

        strip_unsafe_elements(&mut doc);

        let html = doc.outer_html(doc.root());
        assert!(!html.contains("<template"), "template should be removed");
        assert!(!html.contains("onerror"), "onerror should be removed");
        assert!(!html.contains("<script"), "script should be removed");
        assert!(html.contains("Safe content"), "safe content should remain");
    }

    #[test]
    fn test_strip_svg_smil_elements() {
        let mut doc = Document::parse(
            r#"
            <!DOCTYPE html>
            <html>
            <body>
                <svg>
                    <a href="https://safe.com">
                        <animate attributeName="href" to="javascript:alert(1)" />
                        <set attributeName="href" to="javascript:alert(1)" />
                        <animateMotion />
                        <animateTransform />
                        <animateColor />
                        <discard />
                    </a>
                </svg>
                <p>Safe content</p>
            </body>
            </html>
            "#,
        );

        strip_unsafe_elements(&mut doc);

        let html = doc.outer_html(doc.root());
        assert!(!html.contains("<animate"), "animate should be removed");
        assert!(!html.contains("<set"), "set should be removed");
        assert!(!html.contains("<discard"), "discard should be removed");
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
                <img src="data:image/png;base64,AAAA">
                <iframe src="data:image/png;base64,AAAA"></iframe>
                <iframe src="blob:https://example.com/id"></iframe>
                <iframe srcdoc="<script>alert(1)</script>"></iframe>
                <div onclick="doBad()">Click</div>
            </body>
            </html>
            "#,
        );

        strip_unsafe_elements(&mut doc);

        let html = doc.outer_html(doc.root());
        assert!(
            !html.contains("javascript:"),
            "javascript: URLs should be removed"
        );
        assert!(
            !html.contains("data:text/html"),
            "data:text/html URLs should be removed"
        );
        assert_eq!(
            html.matches("data:image/png;base64,AAAA").count(),
            1,
            "inline images should remain on image elements but not iframe src"
        );
        assert!(!html.contains("blob:"), "blob URLs should be removed");
        assert!(!html.contains("srcdoc"), "srcdoc should be removed");
        assert!(!html.contains("onclick"), "onclick should be removed");
        assert!(html.contains("https://safe.com"), "safe URLs should remain");
        assert!(html.contains("Safe link"), "safe link text should remain");
    }

    #[test]
    fn strips_unlisted_event_handler_attributes() {
        let mut doc = Document::parse(
            r#"<div onpointerenter="alert(1)" onanimationstart="alert(2)" data-note="safe">Content</div>"#,
        );

        strip_unsafe_elements(&mut doc);

        let html = doc.outer_html(doc.root());
        assert!(!html.contains("onpointerenter"));
        assert!(!html.contains("onanimationstart"));
        assert!(html.contains("data-note=\"safe\""));
    }
}
