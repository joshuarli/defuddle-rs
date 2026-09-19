use crate::dom::Document;

/// Hoist declarative shadow root content into the light DOM.
///
/// `<template shadowrootmode="open">` (and the older `shadowroot` attribute)
/// declares a shadow root that a browser attaches and renders during parsing.
/// This crate parses static HTML with no browser attached, so that attachment
/// never happens: left alone, the content is either serialized as ordinary
/// (and, after sanitization, removable) `<template>` markup or lost entirely.
/// Unwrapping it here makes the content visible to scoring and extraction the
/// same way a browser would render it.
pub(crate) fn flatten_declarative_shadow_roots(doc: &mut Document) {
    // Hoisting can expose nested declarative shadow roots; bound the traversal depth.
    for _ in 0..10 {
        let candidates = doc.query_selector_all("template[shadowrootmode], template[shadowroot]");
        let mut hoisted_any = false;
        for id in candidates {
            if doc.is_removed(id) {
                continue;
            }
            let mode = doc
                .get_attr(id, "shadowrootmode")
                .or_else(|| doc.get_attr(id, "shadowroot"))
                .unwrap_or("")
                .to_lowercase();
            if mode != "open" && mode != "closed" {
                continue;
            }
            doc.unwrap(id);
            hoisted_any = true;
        }
        if !hoisted_any {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hoists_shadowrootmode_content() {
        let mut doc = Document::parse(
            r#"<!DOCTYPE html><html><body>
            <div><template shadowrootmode="open"><p>Shadow body text.</p></template></div>
            </body></html>"#,
        );
        flatten_declarative_shadow_roots(&mut doc);
        let html = doc.outer_html(doc.root());
        assert!(!html.contains("<template"), "template wrapper should be gone");
        assert!(html.contains("Shadow body text."), "content should be hoisted");
    }

    #[test]
    fn hoists_legacy_shadowroot_attribute() {
        let mut doc = Document::parse(
            r#"<!DOCTYPE html><html><body>
            <div><template shadowroot="open"><p>Legacy shadow text.</p></template></div>
            </body></html>"#,
        );
        flatten_declarative_shadow_roots(&mut doc);
        let html = doc.outer_html(doc.root());
        assert!(!html.contains("<template"));
        assert!(html.contains("Legacy shadow text."));
    }

    #[test]
    fn does_not_hoist_invalid_mode() {
        let mut doc = Document::parse(
            r#"<!DOCTYPE html><html><body>
            <div><template shadowrootmode="bogus"><p>Inert shadow text.</p></template></div>
            </body></html>"#,
        );
        flatten_declarative_shadow_roots(&mut doc);
        let html = doc.outer_html(doc.root());
        assert!(
            html.contains("<template"),
            "invalid mode should remain a plain template"
        );
    }

    #[test]
    fn hoists_nested_declarative_shadow_roots() {
        let mut doc = Document::parse(
            r#"<!DOCTYPE html><html><body>
            <template shadowrootmode="open"><div><template shadowrootmode="open"><p>Nested shadow text.</p></template></div></template>
            </body></html>"#,
        );
        flatten_declarative_shadow_roots(&mut doc);
        let html = doc.outer_html(doc.root());
        assert!(!html.contains("<template"));
        assert!(html.contains("Nested shadow text."));
    }
}
