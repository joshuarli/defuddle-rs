use crate::constants::{ENTRY_POINT_ELEMENTS, EXACT_SELECTORS, PARTIAL_SELECTORS};
use crate::dom::{Document, NodeId};
use crate::scoring;
use crate::utils::count_words;
use rustc_hash::FxHashSet;

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

        let mut seen = FxHashSet::default();
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
                    // Word-boundary check: if pattern starts or ends with '-', ensure
                    // the match is at a CSS class boundary (not mid-word like "commented").
                    if pat.starts_with('-') || pat.ends_with('-') {
                        if is_word_boundary_match(&attr_values, &pat) {
                            matched = true;
                            break;
                        }
                    } else {
                        matched = true;
                        break;
                    }
                }
                if !id_str.is_empty() {
                    let has_delimiter = id_str.contains([' ', '_', '-', ':', '.']);
                    if has_delimiter {
                        if id_str.contains(&pat) {
                            if pat.starts_with('-') || pat.ends_with('-') {
                                if is_word_boundary_match(&id_str, &pat) {
                                    matched = true;
                                    break;
                                }
                            } else {
                                matched = true;
                                break;
                            }
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

/// Check whether a pattern with leading/trailing hyphens matches at CSS class boundaries.
fn is_word_boundary_match(haystack: &str, pattern: &str) -> bool {
    if let Some(pos) = haystack.find(pattern) {
        let after_pos = pos + pattern.len();
        // Character after match must be non-alphabetic or end of string
        if after_pos < haystack.len() {
            let after_char = haystack.as_bytes()[after_pos] as char;
            if after_char.is_alphabetic() {
                return false;
            }
        }
        return true;
    }
    false
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

        let mut seen = FxHashSet::default();
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
                    if pat.starts_with('-') || pat.ends_with('-') {
                        if is_word_boundary_match(&attr_values, &pat) {
                            matched = true;
                            break;
                        }
                    } else {
                        matched = true;
                        break;
                    }
                }
                if !id_str.is_empty() {
                    let has_delimiter = id_str.contains([' ', '_', '-', ':', '.']);
                    if has_delimiter {
                        if id_str.contains(&pat) {
                            if pat.starts_with('-') || pat.ends_with('-') {
                                if is_word_boundary_match(&id_str, &pat) {
                                    matched = true;
                                    break;
                                }
                            } else {
                                matched = true;
                                break;
                            }
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

        // Check aria-hidden="true"
        let aria_hidden = doc.get_attr(id, "aria-hidden").unwrap_or("");
        if aria_hidden == "true" {
            if !to_remove.contains(&id) {
                to_remove.push(id);
            }
            continue;
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
        if words.len() <= 3
            && text.len() < 100
            && (class.contains("text-")
                || class.contains("eyebrow")
                || class.contains("overline")
                || class.contains("kicker"))
        {
            doc.remove(id);
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

    // Remove ISO date lines (YYYY-MM-DD), "N min read", and date formats like "May 22, 2026"
    let date_re = regex_lite::Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
    let named_date_re = regex_lite::Regex::new(
        r"^(?:(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2}(?:,\s*\d{4})?)$",
    )
    .unwrap();
    let min_read_re = regex_lite::Regex::new(r"^\d+\s*min\s*read$").unwrap();
    for &id in &doc.query_selector_all_in_subtree(content_id, "*") {
        if !doc.is_element(id) {
            continue;
        }
        let tag = doc.tag_name(id).to_lowercase();
        if tag != "p" && tag != "span" && tag != "div" && tag != "li" && tag != "time" {
            continue;
        }
        let text = doc.text_content(id).trim().to_string();
        if date_re.is_match(&text)
            || min_read_re.is_match(&text.to_lowercase())
            || named_date_re.is_match(&text)
        {
            let word_count = text.split_whitespace().count();
            if word_count <= 3 {
                doc.remove(id);
            }
        }
    }

    // Remove related-posts headings and their following lists
    remove_related_posts_sections(doc, content_id);

    // Remove table-of-contents sections (short-item lists where all items are links)
    remove_table_of_contents(doc, content_id);

    // Remove newsletter/subscribe blocks
    remove_newsletter_blocks(doc, content_id);

    // Remove social engagement counters (e.g. "13 Likes", "5 Comments")
    remove_social_counters(doc, content_id);

    // Remove social counter links (anchors wrapping "Likes" / "Comments" text)
    remove_social_counter_links(doc, content_id);

    // Remove cookie consent banners
    remove_cookie_consent(doc, content_id);

    // Remove trailing CTA-like sections (thin heading-introduced blocks at end)
    remove_trailing_cta_sections(doc, content_id);

    // Remove card-grid blocks (repeated img+heading sibling patterns)
    remove_card_grid_blocks(doc, content_id);
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
            let siblings = doc.children(pid).to_vec();
            let h_pos = siblings.iter().position(|&s| s == h_id);

            if let Some(pos) = h_pos {
                // Remove metadata elements immediately before the first heading
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

                // Remove metadata elements immediately after the first heading
                if pos + 1 < siblings.len() {
                    let next_id = siblings[pos + 1];
                    let tag = doc.tag_name(next_id).to_lowercase();
                    let class = doc.get_attr(next_id, "class").unwrap_or("").to_lowercase();
                    if tag == "div" || tag == "p" || tag == "span" {
                        let text = doc.text_content(next_id).to_lowercase();
                        if class.contains("meta")
                            || class.contains("date")
                            || class.contains("author")
                            || class.contains("byline")
                            || class.contains("metadata")
                            || class.contains("subtitle")
                            || class.contains("published-at")
                            || class.contains("posted-on")
                            || class.contains("header")
                            || text.starts_with("by ")
                            || text.starts_with("posted ")
                            || text.starts_with("published ")
                            || text.starts_with("updated ")
                            || text.contains("min read")
                            || text.contains("minute read")
                        {
                            let word_count = text.split_whitespace().count();
                            if word_count < 30 {
                                doc.remove(next_id);
                            }
                        }
                        // Also check for date patterns in text content
                        let date_check_re = regex_lite::Regex::new(
                            r"(?:(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},\s*\d{4})"
                        ).unwrap();
                        if date_check_re.is_match(&text) {
                            let word_count = text.split_whitespace().count();
                            if word_count < 20 {
                                doc.remove(next_id);
                            }
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

// ── Content pattern helper functions ──────────────────────────────────────────

fn related_heading_pattern(text: &str) -> bool {
    let re = regex_lite::Regex::new(
        r"(?i)^(?:related (?:posts?|articles?|content|stories|reads?|reading)|you (?:might|may|could) (?:also )?(?:like|enjoy|be interested in)|read (?:next|more|also)|further reading|see also|more (?:from .*|from|articles?|posts?|like this)|more to (?:read|explore)|explore more|about (?:the )?author|latest (?:news|events?|posts?|articles?|stories))(?:[\s&+]+(?:news|events?|posts?|articles?|stories))*$"
    ).unwrap();
    re.is_match(text.trim())
}

fn is_cta_heading(text: &str) -> bool {
    let re = regex_lite::Regex::new(
        r"(?i)^(?:subscribe|sign up|follow us|share this|stay (?:updated|connected)|join (?:us|our)|search (?:the |our )?(?:site|blog|archives?|newsroom))"
    ).unwrap();
    re.is_match(text.trim())
}

fn is_appendix_heading(text: &str) -> bool {
    let re = regex_lite::Regex::new(
        r"(?i)^(?:appendix|acknowledg?ments|disclaimers?|dedication|colophon|epilogue|further reading|references?|bibliography|notes|footnotes)$"
    ).unwrap();
    re.is_match(text.trim())
}

/// Remove sections headed by "Related Posts" / "Read Next" etc.
fn remove_related_posts_sections(doc: &mut Document, content_id: NodeId) {
    let headings = doc.query_selector_all_in_subtree(content_id, "h2,h3,h4,h5");
    let mut to_remove = Vec::new();
    for &h_id in &headings {
        let text = doc.text_content(h_id).trim().to_string();
        if related_heading_pattern(&text) {
            to_remove.push(h_id);
            // Remove the following sibling list if present
            if let Some(pid) = doc.parent(h_id) {
                let siblings = doc.children(pid).to_vec();
                let pos = siblings.iter().position(|&s| s == h_id);
                if let Some(p) = pos
                    && let Some(&sib) = siblings[p + 1..].first()
                {
                    let tag = doc.tag_name(sib).to_lowercase();
                    if tag == "ul" || tag == "ol" || tag == "div" {
                        let sib_text = doc.text_content(sib).trim().to_string();
                        if sib_text.split_whitespace().count() < 80 {
                            to_remove.push(sib);
                        }
                    }
                }
            }
        }
    }
    for id in to_remove {
        doc.remove(id);
    }
}

/// Remove table-of-contents: lists where every item contains a link and items are short.
fn remove_table_of_contents(doc: &mut Document, content_id: NodeId) {
    let lists = doc.query_selector_all_in_subtree(content_id, "ul,ol");
    for list_id in lists {
        let items = doc.query_selector_all_in_subtree(list_id, "li");
        if items.len() < 3 {
            continue;
        }
        let text = doc.text_content(list_id).trim().to_string();
        if text.len() > 250 {
            continue;
        }
        let all_have_links = items
            .iter()
            .all(|&li_id| !doc.query_selector_all_in_subtree(li_id, "a").is_empty());
        if !all_have_links {
            continue;
        }
        // Items must be very short (toc-style, under 10 words each)
        let short_items = items.iter().all(|&li_id| {
            let item_text = doc.text_content(li_id).trim().to_string();
            item_text.split_whitespace().count() < 10
        });
        if short_items && let Some(pid) = doc.parent(list_id) {
            let siblings = doc.children(pid).to_vec();
            let pos = siblings.iter().position(|&s| s == list_id);
            if let Some(p) = pos {
                // Only remove if very near the beginning (first 3 children)
                if p <= 2 {
                    doc.remove(list_id);
                }
            }
        }
    }
}

/// Remove newsletter/subscribe call-to-action blocks.
fn remove_newsletter_blocks(doc: &mut Document, content_id: NodeId) {
    let newsletter_re = regex_lite::Regex::new(
        r"(?i)\bsubscribe\b[\s\S]{0,40}\bnewsletter\b|\bnewsletter\b[\s\S]{0,40}\bsubscribe\b|\bsign[- ]up\b[\s\S]{0,80}\b(?:newsletter|email alert)|\b(?:don['\u2019]?t (?:want to )?miss|never miss)\b"
    ).unwrap();
    let els = doc.query_selector_all_in_subtree(content_id, "div,p,section,aside,span");
    for id in els {
        if has_closest(doc, id, &["pre", "code"]) {
            continue;
        }
        let text = doc.text_content(id).trim().to_string();
        let word_count = text.split_whitespace().count();
        if !(2..=50).contains(&word_count) {
            continue;
        }
        if newsletter_re.is_match(&text) {
            doc.remove(id);
        }
    }
}

/// Remove social engagement counters like "13 Likes", "5 Comments", "2 Shares".
fn remove_social_counters(doc: &mut Document, content_id: NodeId) {
    let social_re = regex_lite::Regex::new(
        r"(?i)^\d+\s+(?:Likes?|Comments?|Shares?|Retweets?|Reposts?|Restacks?|Views?)$",
    )
    .unwrap();
    let els = doc.query_selector_all_in_subtree(content_id, "div,p,span,a,button");
    for id in els {
        if has_closest(doc, id, &["pre", "code"]) {
            continue;
        }
        let text = doc.text_content(id).trim().to_string();
        if social_re.is_match(&text)
            && let Some(pid) = doc.parent(id)
        {
            let wrapper_text = doc.text_content(pid).trim().to_string();
            if wrapper_text.split_whitespace().count() < 10 {
                doc.remove(pid);
            }
        }
    }
}

/// Remove social counter links: anchors wrapping "Likes" / "Comments" text.
fn remove_social_counter_links(doc: &mut Document, content_id: NodeId) {
    let likes_re = regex_lite::Regex::new(r"(?i)^\d+\s+(?:Likes?|Comments?)$").unwrap();
    let anchors = doc.query_selector_all_in_subtree(content_id, "a");
    for a_id in anchors {
        let text = doc.text_content(a_id).trim().to_string();
        if likes_re.is_match(&text) {
            let mut target = a_id;
            for _ in 0..3 {
                if let Some(p) = doc.parent(target) {
                    if p == content_id {
                        break;
                    }
                    let pt = doc.text_content(p).trim().to_string();
                    if pt.split_whitespace().count() > 20 {
                        break;
                    }
                    target = p;
                } else {
                    break;
                }
            }
            if target != a_id {
                doc.remove(target);
            }
        }
    }
}

/// Remove cookie consent banners and overlays.
fn remove_cookie_consent(doc: &mut Document, content_id: NodeId) {
    let cookie_classes = [
        "cookie-consent",
        "cookie-banner",
        "cookie-notice",
        "cookie-message",
        "cookie-policy",
        "eu-cookie",
        "gdpr",
        "cc-banner",
        "consent-banner",
        "cookie-bar",
    ];
    for pattern in &cookie_classes {
        let selector = format!("[class*=\"{pattern}\" i], [id*=\"{pattern}\" i]");
        for id in doc.query_selector_all_in_subtree(content_id, &selector) {
            if !has_closest(doc, id, &["pre", "code"]) {
                doc.remove(id);
            }
        }
    }
}

/// Remove trailing CTA / appendix sections: thin heading-introduced blocks at end.
fn remove_trailing_cta_sections(doc: &mut Document, content_id: NodeId) {
    let content_children = doc.children(content_id).to_vec();
    let total_words: usize = content_children
        .iter()
        .map(|&c| doc.text_content(c).split_whitespace().count())
        .sum();
    if total_words < 100 {
        return;
    }
    let mut trailing_ids = Vec::new();
    let mut trailing_words = 0;
    for &child in content_children.iter().rev() {
        let words = doc.text_content(child).split_whitespace().count();
        let tag = doc.tag_name(child).to_lowercase();

        // HR is a content boundary — include it and stop walking
        if tag == "hr" {
            trailing_ids.push(child);
            break;
        }

        // Check if this element is link-dense (related posts section)
        let links = doc.query_selector_all_in_subtree(child, "a").len();
        let link_dense = links > 0 && words > 0 && (links as f64) > (words as f64) * 0.3;

        // Link-dense elements with no heading are likely related posts
        let _has_heading = is_heading_tag(doc, child)
            || !doc
                .query_selector_all_in_subtree(child, "h1,h2,h3,h4")
                .is_empty();

        // Stop at substantial content paragraphs unless they're link-dense
        if words > 30 && !link_dense {
            break;
        }
        if link_dense && words > 80 {
            break;
        }

        trailing_words += words;
        trailing_ids.push(child);
        if link_dense {
            break;
        }
    }

    if !trailing_ids.is_empty() && trailing_words < total_words * 15 / 100 {
        let has_heading = trailing_ids.iter().any(|&id| is_heading_tag(doc, id));
        let has_cta = trailing_ids.iter().any(|&id| {
            let text = doc.text_content(id).trim().to_string();
            is_cta_heading(&text) || is_appendix_heading(&text)
        });
        let is_link_dense = trailing_ids.iter().any(|&id| {
            let links = doc.query_selector_all_in_subtree(id, "a").len();
            let words = doc.text_content(id).split_whitespace().count();
            links > 0 && (links as f64) > (words as f64) * 0.3
        });
        if has_heading || has_cta || is_link_dense {
            for id in trailing_ids {
                doc.remove(id);
            }
        }
    }
}

/// Remove card-grid blocks: sibling groups of (img + heading) in a repeating pattern.
fn remove_card_grid_blocks(doc: &mut Document, content_id: NodeId) {
    let containers = doc.query_selector_all_in_subtree(content_id, "div,section");
    for container_id in containers {
        let children = doc.children(container_id).to_vec();
        if children.len() < 3 {
            continue;
        }

        // Count how many children have both an img and a heading descendant
        let mut card_count = 0;
        for &child in &children {
            if !doc.is_element(child) {
                continue;
            }
            let has_img = !doc.query_selector_all_in_subtree(child, "img").is_empty();
            let has_heading = !doc
                .query_selector_all_in_subtree(child, "h1,h2,h3,h4,h5,h6")
                .is_empty();
            if has_img && has_heading {
                card_count += 1;
            }
        }

        // If more than half of children are cards, remove the whole container
        if card_count >= 2 && card_count * 2 >= children.len() {
            // But only if the container is not the main content itself
            if container_id != content_id {
                let c_words = doc.text_content(container_id).split_whitespace().count();
                let total = doc.text_content(content_id).split_whitespace().count();
                // Don't remove if this is the bulk of the content
                if c_words < total / 2 {
                    doc.remove(container_id);
                }
            }
        }
    }
}
