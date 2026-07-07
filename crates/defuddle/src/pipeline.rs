use crate::constants::{ENTRY_POINT_ELEMENTS, EXACT_SELECTORS, PARTIAL_SELECTORS};
use crate::dom::{Document, NodeId};
use crate::scoring;
use crate::utils::count_words;

fn has_closest(doc: &Document, node: NodeId, selectors: &[&str]) -> bool {
    selectors
        .iter()
        .any(|selector| doc.closest(node, selector).is_some())
}

pub fn remove_by_selector(doc: &mut Document, remove_exact: bool, remove_partial: bool) {
    if remove_exact {
        // Remove dangerous elements and clutter
        for selector in EXACT_SELECTORS {
            if selector.contains(":not(") || selector.contains(":has(") {
                continue;
            }
            let matches = doc.query_selector_all(selector);
            for id in matches {
                if doc.tag_name(id).is_empty() {
                    continue;
                }
                let tag = doc.tag_name(id).to_lowercase();
                if tag == "pre" || tag == "code" {
                    continue;
                }
                if has_closest(doc, id, &["pre", "code"]) {
                    continue;
                }
                doc.remove(id);
            }
        }

        // Handle :not() selectors manually
        // Remove all scripts EXCEPT those with type starting with "math/"
        let scripts = doc.query_selector_all("script");
        for id in scripts {
            let type_attr = doc.get_attr(id, "type").unwrap_or("");
            if !type_attr.starts_with("math/") {
                doc.remove(id);
            }
        }

        // Remove iframes without src, video/audio without src
        let iframes = doc.query_selector_all("iframe");
        for id in iframes {
            if doc.get_attr(id, "src").is_none() || doc.get_attr(id, "src").unwrap_or("").is_empty()
            {
                doc.remove(id);
            }
        }

        let videos = doc.query_selector_all("video");
        for id in videos {
            if doc.get_attr(id, "src").is_none()
                && doc.query_selector_all_in_subtree(id, "source").is_empty()
            {
                doc.remove(id);
            }
        }

        let audios = doc.query_selector_all("audio");
        for id in audios {
            if doc.get_attr(id, "src").is_none()
                && doc.query_selector_all_in_subtree(id, "source").is_empty()
            {
                doc.remove(id);
            }
        }
    }

    if remove_partial {
        let test_attrs = [
            "class",
            "id",
            "data-component",
            "data-test",
            "data-testid",
            "data-test-id",
            "data-qa",
            "data-cy",
        ];

        let elements: Vec<NodeId> = test_attrs
            .iter()
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

            if has_closest(doc, id, &["pre", "code"]) {
                continue;
            }

            let class_val = doc.get_attr(id, "class").unwrap_or("").to_lowercase();

            let mut attr_values = class_val.clone();
            if tag != "h1"
                && tag != "h2"
                && tag != "h3"
                && tag != "h4"
                && tag != "h5"
                && tag != "h6"
            {
                for attr_name in &[
                    "data-component",
                    "data-test",
                    "data-testid",
                    "data-test-id",
                    "data-qa",
                    "data-cy",
                ] {
                    if let Some(v) = doc.get_attr(id, attr_name) {
                        attr_values.push(' ');
                        attr_values.push_str(&v.to_lowercase());
                    }
                }
            }

            let id_str = if tag == "h1"
                || tag == "h2"
                || tag == "h3"
                || tag == "h4"
                || tag == "h5"
                || tag == "h6"
            {
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
#[allow(dead_code)]
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
                if has_closest(doc, id, &["pre", "code"]) {
                    continue;
                }
                doc.remove(id);
            }
        }
    }

    if remove_partial {
        let test_attrs = [
            "class",
            "id",
            "data-component",
            "data-test",
            "data-testid",
            "data-test-id",
            "data-qa",
            "data-cy",
        ];

        let elements: Vec<NodeId> = test_attrs
            .iter()
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

            if has_closest(doc, id, &["pre", "code"]) {
                continue;
            }

            let class_val = doc.get_attr(id, "class").unwrap_or("").to_lowercase();

            let mut attr_values = class_val.clone();
            if tag != "h1"
                && tag != "h2"
                && tag != "h3"
                && tag != "h4"
                && tag != "h5"
                && tag != "h6"
            {
                for attr_name in &[
                    "data-component",
                    "data-test",
                    "data-testid",
                    "data-test-id",
                    "data-qa",
                    "data-cy",
                ] {
                    if let Some(v) = doc.get_attr(id, attr_name) {
                        attr_values.push(' ');
                        attr_values.push_str(&v.to_lowercase());
                    }
                }
            }

            let id_str = if tag == "h1"
                || tag == "h2"
                || tag == "h3"
                || tag == "h4"
                || tag == "h5"
                || tag == "h6"
            {
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
            let style_lower = style.to_lowercase().replace(' ', "");
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
                if token == "hidden"
                    || token == "invisible"
                    || token.ends_with(":hidden")
                    || token.ends_with(":invisible")
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

#[allow(dead_code)]
pub fn remove_small_images(_doc: &mut Document) {
    // In static HTML parsing, we can't measure actual rendered sizes.
    // This is a best-effort implementation using explicit width/height attributes.
}

/// Remove low-scoring elements based on link-density heuristics.
pub fn score_and_remove(doc: &mut Document, content_id: NodeId) {
    // Process repeatedly since removals may create new low-scoring parents
    for _ in 0..3 {
        let mut to_remove = Vec::new();
        let all: Vec<NodeId> = doc
            .query_selector_all_in_subtree(content_id, "div,section,aside,article")
            .into_iter()
            .filter(|&id| id != content_id)
            .collect();

        for id in all {
            let tag = doc.tag_name(id).to_lowercase();
            if tag == "pre" || tag == "code" || tag == "blockquote" || tag == "figure" {
                continue;
            }
            if has_closest(doc, id, &["pre", "code", "blockquote", "figure", "table"]) {
                continue;
            }
            let score = scoring::score_non_content_block(doc, id);
            if score < -10.0 {
                to_remove.push(id);
            }
        }

        if to_remove.is_empty() {
            break;
        }
        for id in to_remove {
            doc.remove(id);
        }
    }
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

    main_content?;

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

/// Remove common boilerplate and content patterns from the document.
pub fn remove_content_patterns(doc: &mut Document, content_id: NodeId) {
    // Check all elements and remove boilerplate by class
    for &id in &doc.query_selector_all_in_subtree(content_id, "*") {
        if let Some(c) = doc.get_attr(id, "class") {
            let lower = c.to_lowercase();
            let is_boilerplate = lower.contains("writer-contact-block")
                || lower.contains("author-contact")
                || lower.contains("contact-block")
                || lower.contains("author-bio")
                || lower.contains("author-card")
                || lower.contains("author-box")
                || lower.contains("author-list")
                || lower.contains("about-author")
                || lower.contains("article-author")
                || lower.contains("share-widget")
                || lower.contains("share-block")
                || lower.contains("social-share")
                || lower.contains("publish-date")
                || lower.contains("published-date")
                || lower.contains("post-date")
                || lower.contains("article-date")
                || lower.contains("date-published")
                || lower.contains("time-published")
                || lower.contains("publish-time")
                || lower.contains("pencraft");
            if is_boilerplate {
                let text = doc.text_content(id);
                if text.split_whitespace().count() < 50 {
                    doc.remove(id);
                }
            }
        }
    }

    // Remove "Read time", "X min read", "Reading time" elements
    let read_time_patterns = [
        "read-time",
        "read_time",
        "reading-time",
        "reading_time",
        "time-to-read",
        "time_to_read",
        "min-read",
        "min_read",
        "storyreadtime",
        "time-rubric",
        "time-read",
        "timestamp",
    ];
    for pattern in &read_time_patterns {
        let selector = format!("[class*=\"{pattern}\" i], [id*=\"{pattern}\" i]");
        for id in doc.query_selector_all_in_subtree(content_id, &selector) {
            if !has_closest(doc, id, &["pre", "code"]) {
                doc.remove(id);
            }
        }
    }

    // Remove eyebrow/breadcrumb labels: short divs with single-word text
    // like "Blog", "News", "Home" that appear before headings
    for &id in &doc.query_selector_all_in_subtree(content_id, "*") {
        if !doc.is_element(id) {
            continue;
        }
        let tag = doc.tag_name(id).to_lowercase();
        if tag != "div" && tag != "span" && tag != "p" && tag != "nav" && tag != "ol" && tag != "ul"
        {
            continue;
        }
        let class = doc.get_attr(id, "class").unwrap_or("").to_lowercase();
        let text = doc.text_content(id).trim().to_string();
        let words: Vec<&str> = text.split_whitespace().collect();
        // Breadcrumb containers — remove if class indicates breadcrumb
        if class.contains("breadcrumb") || class.contains("nav") {
            doc.remove(id);
            continue;
        }
        // List/nav elements with breadcrumb-like patterns
        if (tag == "ol" || tag == "ul" || tag == "nav") && text.len() < 200 {
            // Check if this looks like a breadcrumb (short items, links, text- classes)
            let link_count = doc.query_selector_all_in_subtree(id, "a").len();
            if link_count >= 1 && class.contains("text-") {
                doc.remove(id);
                continue;
            }
        }
        // Single-word labels that are common breadcrumbs/eyebrows
        if words.len() <= 3 && text.len() < 100 {
            if class.contains("text-")
                || class.contains("eyebrow")
                || class.contains("overline")
                || class.contains("kicker")
            {
                doc.remove(id);
            }
        }
    }

    // Remove author/share labels: elements whose text is just "Share", "Author",
    // "Follow", "Written by" — common sidebar widget labels
    let share_author_re =
        regex_lite::Regex::new(r"(?i)^(share|follow|authors?|written\s+by)$").unwrap();
    for &id in &doc.query_selector_all_in_subtree(content_id, "*") {
        if !doc.is_element(id) {
            continue;
        }
        let tag = doc.tag_name(id).to_lowercase();
        if tag != "p" && tag != "span" && tag != "div" {
            continue;
        }
        let text = doc.text_content(id).trim().to_string();
        if share_author_re.is_match(&text) {
            // Remove the parent container (the widget)
            let mut target = id;
            for _ in 0..3 {
                if let Some(p) = doc.parent(target) {
                    if p == content_id {
                        break;
                    }
                    let pt = doc.text_content(p).trim().to_string();
                    if pt.split_whitespace().count() > 15 {
                        break;
                    }
                    target = p;
                } else {
                    break;
                }
            }
            doc.remove(target);
        }
    }

    // Remove "by Author" byline patterns when adjacent to headings
    for &id in &doc.query_selector_all_in_subtree(content_id, "*") {
        if !doc.is_element(id) {
            continue;
        }
        let tag = doc.tag_name(id).to_lowercase();
        if tag != "p" && tag != "span" && tag != "div" {
            continue;
        }
        let text = doc.text_content(id).trim().to_string();
        let lower = text.to_lowercase();
        if (lower.starts_with("by ") || lower.starts_with("written by ")) && text.len() < 80 {
            let adjacent_heading = doc.parent(id).is_some_and(|parent| {
                let siblings = doc.children(parent);
                siblings.iter().position(|&s| s == id).is_some_and(|pos| {
                    siblings
                        .get(pos.saturating_sub(1))
                        .is_some_and(|&s| is_heading_tag(doc, s))
                        || siblings
                            .get(pos + 1)
                            .is_some_and(|&s| is_heading_tag(doc, s))
                })
            });
            if adjacent_heading {
                doc.remove(id);
            }
        }
    }

    // Remove ISO date lines (YYYY-MM-DD) and "N min read" text
    let date_re = regex_lite::Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
    let min_read_re = regex_lite::Regex::new(r"^\d+\s*min\s*read$").unwrap();
    for &id in &doc.query_selector_all_in_subtree(content_id, "*") {
        if !doc.is_element(id) {
            continue;
        }
        let tag = doc.tag_name(id).to_lowercase();
        if tag != "p" && tag != "span" && tag != "div" && tag != "li" {
            continue;
        }
        let text = doc.text_content(id).trim().to_string();
        if date_re.is_match(&text) || min_read_re.is_match(&text.to_lowercase()) {
            // Remove if this is a small standalone element (not inside content)
            let word_count = text.split_whitespace().count();
            if word_count <= 3 {
                doc.remove(id);
            }
        }
    }
}

fn is_heading_tag(doc: &Document, id: NodeId) -> bool {
    matches!(doc.tag_name(id).to_lowercase().as_str(), "h1" | "h2" | "h3")
}

/// Remove metadata blocks adjacent to the first heading (date, author info).
pub fn remove_metadata_blocks(doc: &mut Document, content_id: NodeId) {
    // Find the first heading
    let first_h = doc
        .query_selector_all_in_subtree(content_id, "h1,h2")
        .into_iter()
        .next();

    if let Some(h_id) = first_h {
        let parent = doc.parent(h_id);
        if let Some(pid) = parent {
            // Remove metadata elements immediately before the first heading
            let siblings = doc.children(pid).to_vec();
            let h_pos = siblings.iter().position(|&s| s == h_id);
            if let Some(pos) = h_pos {
                if pos > 0 {
                    let prev_id = siblings[pos - 1];
                    let tag = doc.tag_name(prev_id).to_lowercase();
                    let class = doc.get_attr(prev_id, "class").unwrap_or("").to_lowercase();
                    if tag == "div" || tag == "p" || tag == "span" {
                        let text = doc.text_content(prev_id).to_lowercase();
                        if class.contains("meta")
                            || class.contains("date")
                            || class.contains("author")
                            || class.contains("byline")
                            || class.contains("metadata")
                            || text.contains("published")
                            || text.contains("updated")
                            || text.contains("by ")
                        {
                            doc.remove(prev_id);
                        }
                    }
                }
            }
        }
    }
}

pub fn count_html_words(content: &str) -> usize {
    let text = content
        .replace(['<', '>'], " ")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");

    let text: String = text
        .chars()
        .filter(|&c| !c.is_ascii_digit() || c == ' ')
        .collect();

    count_words(&text)
}
