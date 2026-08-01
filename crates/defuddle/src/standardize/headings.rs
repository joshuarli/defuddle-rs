use crate::dom::{Document, NodeId};

pub(crate) fn standardize_headings(doc: &mut Document, content_id: NodeId, metadata_title: &str) {
    // Promote h1 → h2 (same as upstream)
    for level in 1..=1 {
        for id in doc.query_selector_all_in_subtree(content_id, &format!("h{level}")) {
            doc.set_tag_name(id, "h2");
        }
    }

    // Remove first h2 if its text matches the metadata title (upstream behavior)
    if !metadata_title.is_empty() {
        let h2s = doc.query_selector_all_in_subtree(content_id, "h2");
        if let Some(&first_h2) = h2s.first() {
            // Strip permalink anchors from heading before comparison
            let permalink_anchors = doc.query_selector_all_in_subtree(
                first_h2,
                "a.headerlink, a.permalink, .headerlink, .permalink",
            );
            for &anchor_id in &permalink_anchors {
                doc.remove(anchor_id);
            }
            let heading_text = crate::content_utils::normalize_text(&doc.text_content(first_h2));
            let normalized_title = crate::content_utils::normalize_text(metadata_title);
            if !normalized_title.is_empty() && heading_text == normalized_title {
                doc.remove(first_h2);
            }
        }
    }
}

pub(crate) fn remove_trailing_headings(doc: &mut Document, content_id: NodeId) {
    let children = doc.children(content_id).to_vec();
    for &child in children.iter().rev() {
        if !doc.is_element(child) {
            if doc.text_content(child).trim().is_empty() {
                continue;
            }
            break;
        }

        if doc.tag_name_is_any(child, &["h1", "h2", "h3", "h4", "h5", "h6"]) {
            if !doc
                .text_content(child)
                .trim()
                .eq_ignore_ascii_case("conclusion")
            {
                doc.remove(child);
            }
            continue;
        }

        if !doc.text_content(child).trim().is_empty() {
            break;
        }
    }
}

pub(crate) fn simplify_heading_children(doc: &mut Document, content_id: NodeId) {
    for level in 1..=6 {
        let headings = doc.query_selector_all_in_subtree(content_id, &format!("h{level}"));
        for h_id in headings {
            for anchor in doc.query_selector_all_in_subtree(h_id, "a") {
                if doc
                    .get_attr(anchor, "href")
                    .is_some_and(|href| href.contains("action=edit"))
                {
                    if let Some(edit_section) = doc.closest(anchor, ".mw-editsection") {
                        doc.remove(edit_section);
                    } else {
                        doc.remove(anchor);
                    }
                }
            }
            let children = doc.children(h_id).to_vec();
            for child in children {
                if !doc.is_element(child) {
                    continue;
                }
                let class = doc.get_attr(child, "class").unwrap_or("");
                // Remove buttons, empty anchors (# links), permalink elements
                if doc.tag_name_is(child, "button") {
                    doc.remove(child);
                } else if doc.tag_name_is(child, "a") {
                    let href = doc.get_attr(child, "href").unwrap_or("");
                    let is_edit_link = href.contains("action=edit");
                    let is_hash_link = href.starts_with('#');
                    if is_hash_link
                        || is_edit_link
                        || crate::content_utils::contains_ascii_case_insensitive(class, "permalink")
                        || crate::content_utils::contains_ascii_case_insensitive(
                            class,
                            "anchor-link",
                        )
                        || crate::content_utils::contains_ascii_case_insensitive(
                            class,
                            "heading-anchor",
                        )
                        || crate::content_utils::contains_ascii_case_insensitive(
                            class,
                            "headerlink",
                        )
                    {
                        let text = doc.text_content(child).trim().to_string();
                        if is_edit_link
                            || text.is_empty()
                            || matches!(text.as_str(), "#" | "¶" | "§")
                        {
                            doc.remove(child);
                        } else {
                            doc.unwrap(child);
                        }
                    }
                }
            }
        }
    }
}
