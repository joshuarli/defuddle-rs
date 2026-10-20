use crate::dom::{Document, NodeId};

pub(crate) fn move_inline_whitespace(doc: &mut Document, content_id: NodeId) {
    let inline = doc
        .query_selector_all_in_subtree(content_id, "code,em,strong,a,mark,s,del");
    for id in inline {
        let Some(parent) = doc.parent(id) else {
            continue;
        };
        let (previous_text, next_text) = {
            let siblings = doc.children(parent);
            let Some(pos) = siblings.iter().position(|&child| child == id) else {
                continue;
            };
            let previous_text = siblings[..pos]
                .iter()
                .rev()
                .copied()
                .find(|&sibling| doc.is_text(sibling));
            let next_text = siblings[pos + 1..]
                .iter()
                .copied()
                .find(|&sibling| {
                    doc.text_node_value(sibling)
                        .is_some_and(|text| !text.trim().is_empty())
                });
            (previous_text, next_text)
        };
        let (first, last) = {
            let children = doc.children(id);
            (children.first().copied(), children.last().copied())
        };
        if let Some(first) = first
            && doc.is_text(first)
        {
            let leading_whitespace = {
                let text = doc.text_node_value(first).expect("text node has a value");
                text.len() - text.trim_start().len()
            };
            if leading_whitespace > 0 {
                if let Some(text) = doc.text_content_mut(first) {
                    text.drain(..leading_whitespace);
                }
                if let Some(previous) = previous_text
                    && let Some(text) = doc.text_content_mut(previous)
                {
                    text.push(' ');
                }
            }
        }
        if let Some(last) = last
            && doc.is_text(last)
        {
            let trailing_whitespace = {
                let text = doc.text_node_value(last).expect("text node has a value");
                text.len() - text.trim_end().len()
            };
            if trailing_whitespace > 0 {
                if let Some(text) = doc.text_content_mut(last) {
                    text.truncate(text.len() - trailing_whitespace);
                }
                if let Some(next) = next_text
                    && let Some(text) = doc.text_content_mut(next)
                {
                    let leading_whitespace = text.len() - text.trim_start().len();
                    if leading_whitespace == 0 {
                        text.insert(0, ' ');
                    } else {
                        text.replace_range(..leading_whitespace, " ");
                    }
                }
            }
        }
        if doc.tag_name(id).eq_ignore_ascii_case("code") {
            let code_text = doc.text_content(id);
            if !code_text.trim().is_empty()
                && let Some(next) = next_text
            {
                let next_text = doc.text_node_value(next).expect("text node has a value");
                if next_text
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric())
                    && code_text
                        .chars()
                        .last()
                        .is_some_and(|c| c.is_alphanumeric())
                    && !next_text.starts_with(char::is_whitespace)
                    && let Some(next_text) = doc.text_content_mut(next)
                {
                    next_text.insert(0, ' ');
                }
            }
        }
    }
}

pub(crate) fn standardize_spaces(doc: &mut Document, node: NodeId) {
    if doc.is_element(node) && doc.tag_name_is_any(node, &["pre", "code", "svg"]) {
        return;
    }

    let children = doc.children(node).to_vec();
    for child in children {
        if doc.is_text(child) {
            if let Some(text) = doc.text_content_mut(child)
                && text.contains('\u{00A0}')
            {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standardizes_nonbreaking_spaces_and_preserves_preformatted_text() {
        let mut doc = Document::parse("<p>a&nbsp;b</p><pre>c&nbsp;d</pre>");
        let root = doc.root();
        standardize_spaces(&mut doc, root);

        let paragraph = doc.query_selector_all("p")[0];
        let preformatted = doc.query_selector_all("pre")[0];
        assert_eq!(doc.text_content(paragraph), "a b");
        assert_eq!(doc.text_content(preformatted), "c\u{00a0}d");
    }
}
