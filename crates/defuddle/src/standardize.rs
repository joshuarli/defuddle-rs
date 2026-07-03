use crate::dom::{Document, NodeId};

pub fn standardize_content(doc: &mut Document, content_id: NodeId) {
    remove_html_comments(doc, content_id);
    flatten_layout_tables(doc, content_id);
    remove_empty_elements(doc, content_id);
}

fn remove_html_comments(doc: &mut Document, node: NodeId) {
    let children = doc.children(node).to_vec();
    for child in children {
        if doc.is_comment(child) {
            doc.remove(child);
        } else if doc.is_element(child) {
            remove_html_comments(doc, child);
        }
    }
}

fn flatten_layout_tables(doc: &mut Document, node: NodeId) {
    let tables = find_descendant_tags(doc, node, "table");
    for table_id in tables {
        let rows = find_descendant_tags(doc, table_id, "tr");
        if rows.len() == 1 {
            let cells = find_descendant_tags(doc, table_id, "td");
            let cells_th = find_descendant_tags(doc, table_id, "th");
            if cells.len() + cells_th.len() == 1 {
                doc.unwrap(table_id);
            }
        }
    }
}

fn remove_empty_elements(doc: &mut Document, node: NodeId) {
    let void_tags: &[&str] = &[
        "area", "base", "br", "col", "embed", "hr", "img", "input",
        "link", "meta", "param", "source", "track", "wbr",
        "svg", "g", "defs", "use", "mask", "pattern",
        "iframe", "video", "audio", "picture",
        "td", "th",
    ];

    let all = doc.query_selector_all("*");
    for el_id in all {
        if !doc.contains(node, el_id) || el_id == node {
            continue;
        }
        let tag = doc.tag_name(el_id).to_lowercase();
        if void_tags.contains(&tag.as_str()) {
            continue;
        }
        let children = doc.children(el_id).to_vec();
        if children.is_empty() {
            let text = doc.text_content(el_id).trim().to_string();
            if text.is_empty() {
                doc.remove(el_id);
            }
        }
    }
}

fn find_descendant_tags(doc: &Document, node: NodeId, tag: &str) -> Vec<NodeId> {
    let mut result = Vec::new();
    find_descendant_tags_into(doc, node, tag, &mut result);
    result
}

fn find_descendant_tags_into(doc: &Document, node: NodeId, tag: &str, result: &mut Vec<NodeId>) {
    for child in doc.children(node).to_vec() {
        if doc.tag_name(child).eq_ignore_ascii_case(tag) {
            result.push(child);
        }
        if !doc.tag_name(child).is_empty() {
            find_descendant_tags_into(doc, child, tag, result);
        }
    }
}
