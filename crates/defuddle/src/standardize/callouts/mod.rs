use crate::dom::Document;

pub(crate) fn convert_alert_elements(doc: &mut Document) {
    convert_bootstrap_alerts(doc);
    convert_hugo_admonitions(doc);
}

fn convert_bootstrap_alerts(doc: &mut Document) {
    for alert_type in ["info", "warning", "danger", "success"] {
        for id in doc.query_selector_all(&format!("div.alert.alert-{alert_type}")) {
            doc.remove_attr(id, "role");
            if let Some(class) = doc.get_attr(id, "class") {
                let class = class
                    .split_whitespace()
                    .filter(|part| *part != "alert" && !part.starts_with("alert-"))
                    .chain(std::iter::once("callout"))
                    .collect::<Vec<_>>()
                    .join(" ");
                doc.set_attr(id, "class", &class);
            }
            doc.set_attr(id, "data-callout", alert_type);

            let children = doc.children(id).to_vec();
            let mut title = String::new();
            for child in children {
                let class = doc
                    .get_attr(child, "class")
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if class.contains("alert-heading") || class.contains("alert-title") {
                    title = doc.text_content(child).trim().to_string();
                    doc.remove(child);
                }
            }
            rebuild_callout(doc, id, &title, doc.children(id).to_vec());
        }
    }
}

fn convert_hugo_admonitions(doc: &mut Document) {
    for kind in ["info", "warning", "danger", "tip", "note", "success"] {
        for id in doc.query_selector_all(&format!("div.admonition.{kind}")) {
            if doc.closest(id, ".callout").is_some() {
                continue;
            }
            let title = doc
                .query_selector_all_in_subtree(id, ".admonition-title")
                .first()
                .map(|&title_id| doc.text_content(title_id).trim().to_string())
                .unwrap_or_default();
            let content = doc
                .query_selector_all_in_subtree(id, ".admonition-content")
                .first()
                .copied();
            let content_children = content
                .map(|content_id| doc.children(content_id).to_vec())
                .unwrap_or_else(|| {
                    doc.children(id)
                        .iter()
                        .copied()
                        .filter(|&child| {
                            let class = doc.get_attr(child, "class").unwrap_or("");
                            !class.contains("admonition-title")
                                && !class.contains("details-summary")
                        })
                        .collect()
                });

            if let Some(class) = doc.get_attr(id, "class") {
                let class = class
                    .split_whitespace()
                    .filter(|part| {
                        *part != "admonition"
                            && *part != "details"
                            && *part != kind
                            && *part != "open"
                    })
                    .chain(std::iter::once("callout"))
                    .collect::<Vec<_>>()
                    .join(" ");
                doc.set_attr(id, "class", &class);
            }
            doc.set_attr(id, "data-callout", kind);

            let old_content_parent = content.and_then(|content_id| doc.parent(content_id));
            for child in doc.children(id).to_vec() {
                if !content_children.contains(&child) && Some(child) != old_content_parent {
                    doc.remove(child);
                }
            }
            rebuild_callout(doc, id, &title, content_children);
            if let Some(content_id) = content {
                doc.remove(content_id);
            }
            if let Some(parent_id) = old_content_parent {
                doc.remove(parent_id);
            }
        }
    }
}

fn rebuild_callout(
    doc: &mut Document,
    id: crate::dom::NodeId,
    title: &str,
    children: Vec<crate::dom::NodeId>,
) {
    let wrapper = if title.is_empty() {
        "<div class=\"callout-content\"></div>"
    } else {
        "<div class=\"callout-title\"><div class=\"callout-title-inner\"></div></div><div class=\"callout-content\"></div>"
    };
    doc.append_html(id, wrapper);
    if !title.is_empty()
        && let Some(title_id) = doc
            .query_selector_all_in_subtree(id, ".callout-title-inner")
            .last()
            .copied()
    {
        doc.append_text(title_id, title);
    }
    if let Some(content_id) = doc
        .query_selector_all_in_subtree(id, ".callout-content")
        .last()
        .copied()
    {
        for child in children {
            if !doc.is_removed(child) {
                doc.append_node(content_id, child);
            }
        }
    }
}
