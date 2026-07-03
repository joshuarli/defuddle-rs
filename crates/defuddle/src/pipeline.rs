use crate::constants::{
    ENTRY_POINT_ELEMENTS, EXACT_SELECTORS, PARTIAL_SELECTORS,
};
use crate::dom::{Document, NodeId};
use crate::scoring;
use crate::utils::count_words;

pub fn remove_by_selector(
    doc: &mut Document,
    remove_exact: bool,
    remove_partial: bool,
) {
    if remove_exact {
        for selector in EXACT_SELECTORS {
            let matches = doc.query_selector_all(selector);
            for id in matches {
                if doc.tag_name(id).is_empty() {
                    continue;
                }
                let tag = doc.tag_name(id).to_lowercase();
                if tag == "pre" || tag == "code" {
                    continue;
                }
                doc.remove(id);
            }
        }
    }

    if remove_partial {
        let test_attrs = [
            "class", "id", "data-component", "data-test", "data-testid",
            "data-test-id", "data-qa", "data-cy",
        ];

        let elements: Vec<NodeId> = test_attrs.iter()
            .flat_map(|attr| doc.query_selector_all(&format!("[{attr}]")))
            .collect();

        let mut seen = std::collections::BTreeSet::new();
        let mut to_remove = Vec::new();

        for id in elements {
            if !seen.insert(id) {
                continue;
            }

            let tag = doc.tag_name(id).to_lowercase();
            if tag == "pre" || tag == "code" {
                continue;
            }

            if doc.closest(id, "pre, code").is_some() {
                continue;
            }

            let class_val = doc.get_attr(id, "class").unwrap_or("").to_lowercase();

            let mut attr_values = class_val.clone();
            if tag != "h1" && tag != "h2" && tag != "h3" && tag != "h4" && tag != "h5" && tag != "h6" {
                for attr_name in &["data-component", "data-test", "data-testid", "data-test-id", "data-qa", "data-cy"] {
                    if let Some(v) = doc.get_attr(id, attr_name) {
                        attr_values.push(' ');
                        attr_values.push_str(&v.to_lowercase());
                    }
                }
            }

            let id_str = if tag == "h1" || tag == "h2" || tag == "h3" || tag == "h4" || tag == "h5" || tag == "h6" {
                String::new()
            } else {
                doc.get_attr(id, "id").unwrap_or("").to_lowercase()
            };

            if attr_values.is_empty() && id_str.is_empty() {
                continue;
            }

            let mut matched = false;
            for pattern in PARTIAL_SELECTORS {
                let pat = pattern.to_lowercase();
                if attr_values.contains(&pat) {
                    matched = true;
                    break;
                }
                if !id_str.is_empty() {
                    let has_delimiter = id_str.contains([' ', '_', '-', ':', '.']);
                    if has_delimiter {
                        if id_str.contains(&pat) {
                            matched = true;
                            break;
                        }
                    } else if id_str == pat {
                        matched = true;
                        break;
                    }
                }
            }

            if matched {
                to_remove.push(id);
            }
        }

        for id in to_remove {
            doc.remove(id);
        }
    }
}

/// Like remove_by_selector but skips ancestors of the content element.
pub fn remove_by_selector_protected(
    doc: &mut Document,
    content_id: NodeId,
    remove_exact: bool,
    remove_partial: bool,
) {
    if remove_exact {
        for selector in EXACT_SELECTORS {
            let matches = doc.query_selector_all(selector);
            for id in matches {
                if doc.tag_name(id).is_empty() {
                    continue;
                }
                if id == content_id || doc.contains(id, content_id) {
                    continue;
                }
                let tag = doc.tag_name(id).to_lowercase();
                if tag == "pre" || tag == "code" {
                    continue;
                }
                doc.remove(id);
            }
        }
    }

    if remove_partial {
        let test_attrs = [
            "class", "id", "data-component", "data-test", "data-testid",
            "data-test-id", "data-qa", "data-cy",
        ];

        let elements: Vec<NodeId> = test_attrs.iter()
            .flat_map(|attr| doc.query_selector_all(&format!("[{attr}]")))
            .collect();

        let mut seen = std::collections::BTreeSet::new();
        let mut to_remove = Vec::new();

        for id in elements {
            if !seen.insert(id) {
                continue;
            }
            if id == content_id || doc.contains(id, content_id) {
                continue;
            }

            let tag = doc.tag_name(id).to_lowercase();
            if tag == "pre" || tag == "code" {
                continue;
            }

            if doc.closest(id, "pre, code").is_some() {
                continue;
            }

            let class_val = doc.get_attr(id, "class").unwrap_or("").to_lowercase();

            let mut attr_values = class_val.clone();
            if tag != "h1" && tag != "h2" && tag != "h3" && tag != "h4" && tag != "h5" && tag != "h6" {
                for attr_name in &["data-component", "data-test", "data-testid", "data-test-id", "data-qa", "data-cy"] {
                    if let Some(v) = doc.get_attr(id, attr_name) {
                        attr_values.push(' ');
                        attr_values.push_str(&v.to_lowercase());
                    }
                }
            }

            let id_str = if tag == "h1" || tag == "h2" || tag == "h3" || tag == "h4" || tag == "h5" || tag == "h6" {
                String::new()
            } else {
                doc.get_attr(id, "id").unwrap_or("").to_lowercase()
            };

            if attr_values.is_empty() && id_str.is_empty() {
                continue;
            }

            let mut matched = false;
            for pattern in PARTIAL_SELECTORS {
                let pat = pattern.to_lowercase();
                if attr_values.contains(&pat) {
                    matched = true;
                    break;
                }
                if !id_str.is_empty() {
                    let has_delimiter = id_str.contains([' ', '_', '-', ':', '.']);
                    if has_delimiter {
                        if id_str.contains(&pat) {
                            matched = true;
                            break;
                        }
                    } else if id_str == pat {
                        matched = true;
                        break;
                    }
                }
            }

            if matched {
                to_remove.push(id);
            }
        }

        for id in to_remove {
            doc.remove(id);
        }
    }
}

pub fn remove_hidden_elements(doc: &mut Document) {
    let all_elements = doc.query_selector_all("*");
    let mut to_remove = Vec::new();

    for id in all_elements {
        let tag = doc.tag_name(id).to_lowercase();
        if tag == "math" {
            continue;
        }

        let style = doc.get_attr(id, "style").unwrap_or("");
        if !style.is_empty() {
            let style_lower = style.to_lowercase()
                .replace(' ', "");
            if style_lower.contains("display:none")
                || style_lower.contains("visibility:hidden")
                || style_lower.contains("opacity:0")
            {
                if !to_remove.contains(&id) {
                    to_remove.push(id);
                }
                continue;
            }
        }

        let class_val = doc.get_attr(id, "class").unwrap_or("");
        if !class_val.is_empty() {
            let tokens: Vec<&str> = class_val.split_whitespace().collect();
            for token in tokens {
                if token == "hidden" || token == "invisible"
                    || token.ends_with(":hidden") || token.ends_with(":invisible")
                {
                    if !to_remove.contains(&id) {
                        to_remove.push(id);
                    }
                    break;
                }
            }
        }
    }

    for id in to_remove {
        doc.remove(id);
    }
}

pub fn remove_small_images(_doc: &mut Document) {
    // In static HTML parsing, we can't measure actual rendered sizes.
    // This is a best-effort implementation using explicit width/height attributes.
}

pub fn extract_content(
    doc: &mut Document,
    content_selector: Option<&str>,
    _debug: bool,
) -> Option<NodeId> {
    let main_content = if let Some(selector) = content_selector {
        doc.query_selector_all(selector).into_iter().next()
    } else {
        scoring::find_main_content(doc, ENTRY_POINT_ELEMENTS)
    };

    if main_content.is_none() {
        return None;
    }

    let main = main_content.unwrap();

    let tag = doc.tag_name(main).to_lowercase();
    if tag == "body" {
        // Try to find a more specific element
        if let Some(specific) = scoring::find_main_content(doc, ENTRY_POINT_ELEMENTS) {
            let specific_tag = doc.tag_name(specific).to_lowercase();
            if specific_tag != "body" && specific != main {
                return Some(specific);
            }
        }
    }

    Some(main)
}

pub fn count_html_words(content: &str) -> usize {
    let text = content
        .replace(|c: char| c == '<' || c == '>', " ")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");

    let text: String = text.chars()
        .filter(|&c| !c.is_ascii_digit() || c == ' ')
        .collect();

    count_words(&text)
}
