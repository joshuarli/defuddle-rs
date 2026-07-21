use crate::dom::{Document, NodeId};

pub(crate) fn move_inline_whitespace(doc: &mut Document, content_id: NodeId) {
    let inline = doc
        .query_selector_all_in_subtree(content_id, "code,em,strong,a,mark,s,del")
        .into_iter()
        .collect::<Vec<_>>();
    for id in inline {
        let Some(parent) = doc.parent(id) else {
            continue;
        };
        let siblings = doc.children(parent).to_vec();
        let Some(pos) = siblings.iter().position(|&child| child == id) else {
            continue;
        };
        let children = doc.children(id).to_vec();
        if let Some(&first) = children.first()
            && doc.is_text(first)
        {
            let text = doc.text_content(first).to_string();
            let trimmed = text.trim_start().to_string();
            if trimmed != text {
                doc.set_text(first, &trimmed);
                if let Some(&previous) = siblings[..pos].iter().rev().find(|&&s| doc.is_text(s)) {
                    let value = format!("{} ", doc.text_content(previous));
                    doc.set_text(previous, &value);
                }
            }
        }
        if let Some(&last) = children.last()
            && doc.is_text(last)
        {
            let text = doc.text_content(last).to_string();
            let trimmed = text.trim_end().to_string();
            if trimmed != text {
                doc.set_text(last, &trimmed);
                if let Some(&next) = siblings[pos + 1..]
                    .iter()
                    .find(|&&s| doc.is_text(s) && !doc.text_content(s).trim().is_empty())
                {
                    let value = format!(" {}", doc.text_content(next).trim_start());
                    doc.set_text(next, &value);
                }
            }
        }
        if doc.tag_name(id).eq_ignore_ascii_case("code") {
            let code_text = doc.text_content(id).trim().to_string();
            if !code_text.is_empty()
                && let Some(&next) = siblings[pos + 1..]
                    .iter()
                    .find(|&&s| doc.is_text(s) && !doc.text_content(s).trim().is_empty())
            {
                let next_text = doc.text_content(next).to_string();
                if next_text
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric())
                    && code_text
                        .chars()
                        .last()
                        .is_some_and(|c| c.is_alphanumeric())
                    && !next_text.starts_with(char::is_whitespace)
                {
                    doc.set_text(next, &format!(" {next_text}"));
                }
            }
        }
    }
}

pub(crate) fn standardize_spaces(doc: &mut Document, node: NodeId) {
    // Skip pre, code, and SVG elements to preserve whitespace inside code blocks and SVGs
    if doc.is_element(node) {
        if doc.tag_name_is_any(node, &["pre", "code", "svg"]) {
            return;
        }
    }

    let children = doc.children(node).to_vec();
    for child in children {
        if doc.is_text(child) {
            if let Some(text) = doc.text_content_mut(child) {
                // Only replace non-breaking spaces — do NOT collapse consecutive spaces
                *text = text.replace('\u{00A0}', " ");
            }
        } else if doc.is_element(child) {
            standardize_spaces(doc, child);
        }
    }
}


pub(crate) fn remove_html_comments(doc: &mut Document, node: NodeId) {
    let children = doc.children(node).to_vec();
    for child in children {
        if doc.is_comment(child) {
            doc.remove(child);
        } else if doc.is_element(child) {
            remove_html_comments(doc, child);
        }
    }
}

