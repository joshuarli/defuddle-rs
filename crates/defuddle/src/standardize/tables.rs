use crate::dom::{Document, NodeId};
use crate::extraction_selectors::BLOCK_LEVEL_ELEMENTS;

pub(crate) fn normalize_data_tables(doc: &mut Document, content_id: NodeId) {
    for table_id in doc.query_selector_all_in_subtree(content_id, "table") {
        if !doc.query_selector_all_in_subtree(table_id, "th").is_empty() {
            continue;
        }
        let rows = doc.query_selector_all_in_subtree(table_id, "tr");
        let Some(first_row) = rows.first().copied() else {
            continue;
        };
        for cell in doc.children(first_row).to_vec() {
            if doc.is_element(cell) && doc.tag_name(cell).eq_ignore_ascii_case("td") {
                doc.set_tag_name(cell, "th");
            }
        }
    }
}

pub(crate) fn unwrap_layout_tables(doc: &mut Document, content_id: NodeId) {
    let mut tables = doc.query_selector_all_in_subtree(content_id, "table");
    if doc.is_element(content_id) && doc.tag_name(content_id).eq_ignore_ascii_case("table") {
        tables.insert(0, content_id);
    }
    for table_id in tables {
        let has_structural = doc.query_selector_all_in_subtree(table_id, "thead,tfoot,th,caption");
        if !has_structural.is_empty() {
            continue;
        }
        let all_cells = doc.query_selector_all_in_subtree(table_id, "td");
        let non_empty: Vec<NodeId> = all_cells
            .into_iter()
            .filter(|&cid| !doc.text_content(cid).trim().is_empty())
            .collect();
        if non_empty.len() != 1 {
            continue;
        }
        let cell = non_empty[0];
        let inner = doc.inner_html(cell);
        if !inner.trim().is_empty() {
            doc.replace_with_html(table_id, &inner);
            continue;
        }
        let cell_children: Vec<NodeId> = doc
            .children(cell)
            .to_vec()
            .into_iter()
            .filter(|&cid| doc.is_element(cid) && !doc.text_content(cid).trim().is_empty())
            .collect();
        if cell_children.len() == 1 {
            let child = cell_children[0];
            let child_tag = doc.tag_name(child);
            if BLOCK_LEVEL_ELEMENTS
                .iter()
                .any(|tag| child_tag.eq_ignore_ascii_case(tag))
            {
                doc.replace_with_node(table_id, child);
            }
        } else if cell_children.len() > 1 {
            // Multiple block children in the cell: flatten the layout table,
            // replacing it with the cell's inner HTML.
            let inner = doc.inner_html(cell);
            doc.replace_with_html(table_id, &inner);
        }
    }
}
