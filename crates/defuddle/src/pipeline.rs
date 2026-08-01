use crate::content_utils::count_words;
use crate::dom::{Document, NodeId};
use crate::extraction_selectors::{ENTRY_POINT_ELEMENTS, EXACT_SELECTORS, PARTIAL_SELECTORS};
use crate::scoring;
use rustc_hash::FxHashSet;
use std::sync::OnceLock;

use crate::{DefuddleOptions, MetaTag, Metadata};

#[derive(Debug, Clone, Default)]
pub(crate) struct Diagnostics {
    pub phases: Vec<PhaseTiming>,
}

#[derive(Debug, Clone)]
pub(crate) struct PhaseTiming {
    pub name: &'static str,
    pub elapsed_nanos: u128,
}

impl Diagnostics {
    pub(crate) fn measure<T>(&mut self, name: &'static str, operation: impl FnOnce() -> T) -> T {
        let (result, elapsed_nanos) = timed(operation);
        self.phases.push(PhaseTiming {
            name,
            elapsed_nanos,
        });
        result
    }

    pub(crate) fn total_nanos(&self) -> u128 {
        self.phases
            .iter()
            .filter(|phase| !phase.name.is_empty())
            .map(|phase| phase.elapsed_nanos)
            .sum()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn report_if_requested(&self) {
        if std::env::var_os("DEFUDDLE_PROFILE").is_none() {
            return;
        }
        eprintln!("defuddle profile:");
        for phase in &self.phases {
            eprintln!(
                "  {:<24} {:>8.3} ms",
                phase.name,
                phase.elapsed_nanos as f64 / 1_000_000.0
            );
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn report_if_requested(&self) {}
}

pub(crate) fn timed<T>(operation: impl FnOnce() -> T) -> (T, u128) {
    #[cfg(target_arch = "wasm32")]
    {
        (operation(), 0)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let start = std::time::Instant::now();
        let result = operation();
        (result, start.elapsed().as_nanos())
    }
}

pub(crate) struct ExtractionContext<'a> {
    pub source_html: &'a str,
    pub doc: Document,
    pub metadata: &'a Metadata,
    pub meta_tags: &'a [MetaTag],
    pub schema_org_data: &'a [miniserde::json::Value],
    pub page_url: &'a str,
    pub defuddle_options: &'a DefuddleOptions,
    pub diagnostics: Diagnostics,
}

pub(crate) struct ExtractedContent {
    pub document: Document,
    pub root: NodeId,
}

impl ExtractedContent {
    pub(crate) fn html(&self) -> String {
        self.document.outer_html(self.root)
    }
}

static RELATED_HEADING_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static CTA_HEADING_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static APPENDIX_HEADING_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static SHARE_AUTHOR_LABEL_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static ISO_DATE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static NAMED_DATE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static MIN_READ_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static COMPACT_DATE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static DATE_CHECK_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static NEWSLETTER_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static SOCIAL_COUNTER_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static LIKES_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static PARTIAL_SELECTOR_PATTERNS: OnceLock<Vec<String>> = OnceLock::new();

fn partial_selector_patterns() -> &'static [String] {
    PARTIAL_SELECTOR_PATTERNS.get_or_init(|| {
        PARTIAL_SELECTORS
            .iter()
            .map(|pattern| pattern.to_lowercase())
            .collect()
    })
}

pub fn remove_by_selector(doc: &mut Document, remove_exact: bool, remove_partial: bool) {
    let mut matches = Vec::new();
    if remove_exact {
        // Remove dangerous elements and clutter
        let selectors = EXACT_SELECTORS
            .iter()
            .copied()
            .filter(|selector| {
                // Multiple :not() and :has() compound selectors are not fully
                // supported yet. Skip the few that need compound negation.
                !selector.contains(":not(") || selector.matches(":not(").count() <= 1
            })
            .filter(|selector| !selector.contains(":has("))
            .collect::<Vec<_>>();
        doc.query_selector_union_into(&selectors, &mut matches);
        for &id in &matches {
            if doc.tag_name(id).is_empty() {
                continue;
            }
            if doc.tag_name_is_any(id, &["pre", "code"]) {
                continue;
            }
            if doc.has_closest(id, &["pre", "code"]) {
                continue;
            }
            doc.remove(id);
        }

        // Manual handling for multi-:not()/:has() compound selectors
        // Remove all scripts.
        doc.query_selector_all_into("script", &mut matches);
        for &id in &matches {
            doc.remove(id);
        }
        // Remove iframes without src or with empty src
        doc.query_selector_all_into("iframe", &mut matches);
        for &id in &matches {
            if doc.get_attr(id, "src").is_none_or(|s| s.is_empty()) {
                doc.remove(id);
            }
        }
        // Remove video/audio without src and without <source> children
        doc.query_selector_all_into("video", &mut matches);
        for &id in &matches {
            if doc.get_attr(id, "src").is_none() && !doc.has_selector_in_subtree(id, "source") {
                doc.remove(id);
            }
        }
        doc.query_selector_all_into("audio", &mut matches);
        for &id in &matches {
            if doc.get_attr(id, "src").is_none() && !doc.has_selector_in_subtree(id, "source") {
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

        let attribute_selectors = test_attrs.map(|attr| format!("[{attr}]"));
        let attribute_selectors = attribute_selectors
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let mut elements = Vec::new();
        doc.query_selector_union_into(&attribute_selectors, &mut elements);

        let mut seen = FxHashSet::default();
        let mut to_remove = Vec::new();

        for id in elements {
            if !seen.insert(id) {
                continue;
            }

            if doc.tag_name_is_any(id, &["pre", "code"]) {
                continue;
            }

            if doc.has_closest(id, &["pre", "code"]) {
                continue;
            }

            let class_val = doc.get_attr(id, "class").unwrap_or("").to_lowercase();

            let mut attr_values = class_val.clone();
            if !doc.tag_name_is_any(id, &["h1", "h2", "h3", "h4", "h5", "h6"]) {
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

            let id_str = if doc.tag_name_is_any(id, &["h1", "h2", "h3", "h4", "h5", "h6"]) {
                String::new()
            } else {
                doc.get_attr(id, "id").unwrap_or("").to_lowercase()
            };

            if attr_values.is_empty() && id_str.is_empty() {
                continue;
            }

            let mut matched = false;
            for pat in partial_selector_patterns() {
                if attr_values.contains(pat) {
                    // `logo` is common inside unrelated names such as
                    // `a-content--logo-end`; match it only at a token edge.
                    // Hyphen-prefixed/suffixed patterns use the same boundary
                    // rule to avoid removing similarly named content blocks.
                    if pat == "logo" {
                        if is_token_edge_match(&attr_values, &pat) {
                            matched = true;
                            break;
                        }
                    } else if pat.starts_with('-') || pat.ends_with('-') {
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
                        if id_str.contains(pat) {
                            if pat == "logo" {
                                if is_token_edge_match(&id_str, &pat) {
                                    matched = true;
                                    break;
                                }
                            } else if pat.starts_with('-') || pat.ends_with('-') {
                                if is_word_boundary_match(&id_str, &pat) {
                                    matched = true;
                                    break;
                                }
                            } else {
                                matched = true;
                                break;
                            }
                        }
                    } else if id_str == pat.as_str() {
                        matched = true;
                        break;
                    }
                }
            }

            if matched {
                if attr_values.contains("reference-accessdate") {
                    continue;
                }
                if attr_values.contains("author-bio") && {
                    let mut ancestor = doc.parent(id);
                    let mut inside_article = false;
                    while let Some(current) = ancestor {
                        if doc.tag_name(current).eq_ignore_ascii_case("article") {
                            inside_article = true;
                            break;
                        }
                        ancestor = doc.parent(current);
                    }
                    inside_article
                } {
                    continue;
                }
                if attr_values.contains("sidebar-content")
                    && crate::content_utils::count_words(&doc.text_content(id)) >= 80
                    && doc.has_selector_in_subtree(id, "p")
                {
                    continue;
                }
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

fn is_token_edge_match(haystack: &str, pattern: &str) -> bool {
    let mut offset = 0;
    while let Some(relative_pos) = haystack[offset..].find(pattern) {
        let pos = offset + relative_pos;
        let end = pos + pattern.len();
        let before_is_word_or_hyphen = pos > 0
            && haystack[..pos]
                .chars()
                .next_back()
                .is_some_and(|character| {
                    character.is_ascii_alphanumeric() || character == '_' || character == '-'
                });
        let after_is_word_or_hyphen = end < haystack.len()
            && haystack[end..].chars().next().is_some_and(|character| {
                character.is_ascii_alphanumeric() || character == '_' || character == '-'
            });
        if !before_is_word_or_hyphen || !after_is_word_or_hyphen {
            return true;
        }
        offset = end;
    }
    false
}
pub fn remove_hidden_elements(doc: &mut Document) {
    let all_elements = doc.query_selector_all("*");
    let mut to_remove = Vec::new();

    for id in all_elements {
        let style = doc.get_attr(id, "style").unwrap_or("");
        if doc.get_attr(id, "hidden").is_some() {
            to_remove.push(id);
            continue;
        }
        if !style.is_empty() {
            let style_lower = style.to_lowercase().replace(' ', "");
            if style_lower.contains("display:none")
                || style_lower.contains("visibility:hidden")
                || style_lower.contains("opacity:0")
            {
                if doc.closest(id, "span.inline-footnote").is_some() {
                    continue;
                }
                // Skip elements inside callout containers (e.g. collapsed callout content)
                if doc.closest(id, "[data-callout]").is_none()
                    && doc.closest(id, ".callout").is_none()
                    && !to_remove.contains(&id)
                {
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
        if let Some(parent) = doc.parent(id) {
            let siblings = doc.children(parent).to_vec();
            if let Some(pos) = siblings.iter().position(|&s| s == id)
                && let Some(&previous) = siblings[..pos].iter().rev().find(|&&s| doc.is_element(s))
                && matches!(
                    doc.tag_name(previous).to_ascii_lowercase().as_str(),
                    "h2" | "h3" | "h4" | "h5" | "h6"
                )
            {
                doc.remove(previous);
            }
        }
        doc.remove(id);
    }
}
/// Remove low-scoring elements based on link-density heuristics.
pub fn score_and_remove(doc: &mut Document, content_id: NodeId) {
    // Process repeatedly since removals may create new low-scoring parents
    for _ in 0..3 {
        let mut to_remove = Vec::new();
        let mut text = String::new();
        let all: Vec<NodeId> = doc
            .query_selector_all_in_subtree(content_id, "div,section,aside,article")
            .into_iter()
            .filter(|&id| id != content_id)
            .collect();

        for id in all {
            if doc.tag_name_is_any(id, &["pre", "code", "blockquote", "figure"]) {
                continue;
            }
            if doc.has_closest(id, &["pre", "code", "blockquote", "figure", "table"]) {
                continue;
            }
            if doc.has_selector_in_subtree(id, ".gated-content, [data-schema=smart-brevity]") {
                continue;
            }
            if doc
                .get_attr(id, "class")
                .unwrap_or("")
                .split_whitespace()
                .any(|class| class.eq_ignore_ascii_case("author-bio"))
                && doc.closest(id, "article").is_some()
            {
                continue;
            }
            text.clear();
            doc.text_content_into(id, &mut text);
            if doc.count_selector_in_subtree(id, "img") == 1
                && doc.has_selector_in_subtree(id, "a")
                && text.split_whitespace().count() < 30
            {
                continue;
            }
            if doc
                .query_selector_all_in_subtree(id, "img")
                .iter()
                .any(|img| {
                    doc.get_attr(*img, "alt")
                        .is_some_and(|alt| !alt.trim().is_empty())
                        && doc.closest(*img, "a").is_some_and(|a| {
                            doc.get_attr(a, "href")
                                .is_some_and(|href| href.contains("/@"))
                        })
                })
            {
                continue;
            }
            let score = scoring::score_non_content_block_with_text(doc, id, &text);
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

    let Some(main) = main_content else {
        return None;
    };

    if doc.tag_name_is(main, "body") {
        if let Some(main_element) = doc.query_selector_all("main").first().copied()
            && count_html_words(&doc.outer_html(main_element)) > 100
        {
            return Some(main_element);
        }
        // Try to find a more specific element
        if let Some(specific) = scoring::find_main_content(doc, ENTRY_POINT_ELEMENTS)
            && !doc.tag_name_is(specific, "body")
            && specific != main
        {
            return Some(specific);
        }
    }

    Some(main)
}

/// Remove "eyebrow" elements — short category/taxonomy labels immediately preceding
/// the first heading (e.g. "Blog post", "Off-nominal", "Announcements").
fn remove_eyebrow_label(doc: &mut Document, content_id: NodeId) {
    // Find first h1 or h2
    let first_heading = doc
        .query_selector_all_in_subtree(content_id, "h1, h2")
        .first()
        .copied();
    let Some(first_h) = first_heading else { return };

    // Walk up through wrappers where the heading is the first child
    let mut current = first_h;
    while let Some(parent) = doc.parent(current) {
        if parent == content_id {
            break;
        }
        let children = doc.children(parent);
        if children.first() != Some(&current) {
            break;
        }
        current = parent;
    }

    // Find previous element sibling (skip text/whitespace nodes)
    let prev = {
        let parent = doc.parent(current);
        let children = doc.children(parent.unwrap_or(content_id));
        let pos = children.iter().position(|&c| c == current);
        pos.and_then(|p| {
            children[..p]
                .iter()
                .rev()
                .find(|&&c| doc.is_element(c))
                .copied()
        })
    };
    let Some(prev) = prev else { return };
    let text = doc.text_content(prev).trim().to_string();
    let word_count = text.split_whitespace().count();
    if word_count == 0 || word_count > 6 {
        return;
    }
    if text.len() > 40 {
        return;
    }
    if text.contains(['.', '!', '?']) {
        return;
    }
    // Don't remove if it contains substantive elements
    let substantive = doc.query_selector_all_in_subtree(
        prev,
        "img, picture, video, iframe, figure, table, pre, code, time, [datetime], h1, h2, h3, h4, h5, h6, ul, ol, blockquote",
    );
    if !substantive.is_empty() {
        return;
    }

    doc.remove(prev);
}

/// Remove common boilerplate and content patterns from the document.
pub fn remove_content_patterns(doc: &mut Document, content_id: NodeId) {
    let mut matches = Vec::new();
    let mut text = String::new();

    // Remove eyebrow/category labels before the first heading
    remove_eyebrow_label(doc, content_id);
    doc.query_selector_all_in_subtree_into(content_id, ".abstract", &mut matches);
    for &abstract_node in &matches {
        doc.remove(abstract_node);
    }
    // Check all elements and remove boilerplate by class
    doc.query_selector_all_in_subtree_into(content_id, "*", &mut matches);
    for &id in &matches {
        if let Some(c) = doc.get_attr(id, "class") {
            let is_boilerplate = [
                "writer-contact-block",
                "author-contact",
                "contact-block",
                "author-card",
                "author-box",
                "author-list",
                "about-author",
                "article-author",
                "share-widget",
                "share-block",
                "social-share",
                "publish-date",
                "published-date",
                "post-date",
                "article-date",
                "date-published",
                "time-published",
                "publish-time",
                "pencraft",
            ]
            .iter()
            .any(|pattern| crate::content_utils::contains_ascii_case_insensitive(c, pattern));
            if is_boilerplate {
                text.clear();
                doc.text_content_into(id, &mut text);
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
        doc.query_selector_all_in_subtree_into(content_id, &selector, &mut matches);
        for &id in &matches {
            if !doc.has_closest(id, &["pre", "code"]) {
                doc.remove(id);
            }
        }
    }

    // Remove eyebrow/breadcrumb labels: short divs with single-word text
    // like "Blog", "News", "Home" that appear before headings
    doc.query_selector_all_in_subtree_into(content_id, "*", &mut matches);
    for &id in &matches {
        if !doc.is_element(id) {
            continue;
        }
        let tag = doc.tag_name(id);
        if !matches!(tag, "div" | "span" | "p" | "nav" | "ol" | "ul") {
            continue;
        }
        let class = doc.get_attr(id, "class").unwrap_or("");
        text.clear();
        doc.text_content_into(id, &mut text);
        let text = text.trim();
        let word_count = text.split_whitespace().count();
        // Breadcrumb containers — remove if class indicates breadcrumb
        if crate::content_utils::contains_ascii_case_insensitive(class, "breadcrumb")
            || crate::content_utils::contains_ascii_case_insensitive(class, "nav")
        {
            doc.remove(id);
            continue;
        }
        // List/nav elements with breadcrumb-like patterns
        if (tag == "ol" || tag == "ul" || tag == "nav") && text.len() < 200 {
            // Check if this looks like a breadcrumb (short items, links, text- classes)
            let link_count = doc.count_selector_in_subtree(id, "a");
            if link_count >= 1
                && crate::content_utils::contains_ascii_case_insensitive(class, "text-")
            {
                doc.remove(id);
                continue;
            }
        }
        // Single-word labels that are common breadcrumbs/eyebrows
        if word_count <= 3
            && text.len() < 100
            && (crate::content_utils::contains_ascii_case_insensitive(class, "text-")
                || crate::content_utils::contains_ascii_case_insensitive(class, "eyebrow")
                || crate::content_utils::contains_ascii_case_insensitive(class, "overline")
                || crate::content_utils::contains_ascii_case_insensitive(class, "kicker"))
        {
            doc.remove(id);
        }
    }

    // Remove author/share labels: elements whose text is just "Share", "Author",
    // "Follow", "Written by" — common sidebar widget labels
    let share_author_re = SHARE_AUTHOR_LABEL_RE.get_or_init(|| {
        regex_lite::Regex::new(r"(?i)^(share|follow|authors?|written\s+by)$").unwrap()
    });
    doc.query_selector_all_in_subtree_into(content_id, "*", &mut matches);
    for &id in &matches {
        if !doc.is_element(id) {
            continue;
        }
        let tag = doc.tag_name(id);
        if !matches!(tag, "p" | "span" | "div" | "time" | "relative-time") {
            continue;
        }
        text.clear();
        doc.text_content_into(id, &mut text);
        let text_value = text.trim();
        if share_author_re.is_match(text_value) {
            // Remove the parent container (the widget)
            let mut target = id;
            for _ in 0..3 {
                if let Some(p) = doc.parent(target) {
                    if p == content_id {
                        break;
                    }
                    text.clear();
                    doc.text_content_into(p, &mut text);
                    let pt = text.trim();
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
    doc.query_selector_all_in_subtree_into(content_id, "*", &mut matches);
    for &id in &matches {
        if !doc.is_element(id) {
            continue;
        }
        let tag = doc.tag_name(id);
        if !matches!(tag, "p" | "span" | "div") {
            continue;
        }
        let class = doc.get_attr(id, "class").unwrap_or("");
        // Remove subtitle/lede paragraphs near beginning of content
        if crate::content_utils::contains_ascii_case_insensitive(class, "subtitle")
            || crate::content_utils::contains_ascii_case_insensitive(class, "lede")
            || crate::content_utils::contains_ascii_case_insensitive(class, "deck")
            || crate::content_utils::contains_ascii_case_insensitive(class, "article-deck")
        {
            text.clear();
            doc.text_content_into(id, &mut text);
            let text = text.trim();
            let word_count = text.split_whitespace().count();
            if word_count < 40 {
                doc.remove(id);
                continue;
            }
        }
        text.clear();
        doc.text_content_into(id, &mut text);
        let text = text.trim();
        if text.len() < 80
            && (text.get(..3).is_some_and(|s| s.eq_ignore_ascii_case("by "))
                || text
                    .get(..11)
                    .is_some_and(|s| s.eq_ignore_ascii_case("written by ")))
        {
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
    let date_re =
        ISO_DATE_RE.get_or_init(|| regex_lite::Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());
    let named_date_re = NAMED_DATE_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"^(?:(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2}(?:,\s*\d{4})?|\d{1,2}\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*(?:,\s*\d{4})?)$",
        )
        .unwrap()
    });
    let min_read_re =
        MIN_READ_RE.get_or_init(|| regex_lite::Regex::new(r"^\d+\s*min\s*read$").unwrap());
    let compact_read_meta_re = COMPACT_DATE_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},?\s+\d{4}",
        )
        .unwrap()
    });
    doc.query_selector_all_in_subtree_into(content_id, "*", &mut matches);
    for &id in &matches {
        if !doc.is_element(id) {
            continue;
        }
        let tag = doc.tag_name(id);
        if !matches!(tag, "p" | "span" | "div" | "li" | "time") {
            continue;
        }
        text.clear();
        doc.text_content_into(id, &mut text);
        let text = text.trim();
        let compact_read_meta =
            crate::content_utils::contains_ascii_case_insensitive(text, "min read")
                && text.split_whitespace().count() <= 12
                && compact_read_meta_re.is_match(text);
        if date_re.is_match(text)
            || min_read_re.is_match(text)
            || named_date_re.is_match(text)
            || compact_read_meta
        {
            if doc.closest(id, ".reference-accessdate").is_some() {
                continue;
            }
            let word_count = text.split_whitespace().count();
            if word_count <= 3 || compact_read_meta {
                doc.remove(id);
            }
        }
    }

    // Remove byline blocks (author link + date near article top)
    remove_byline_blocks(doc, content_id);

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
    doc.tag_name_is_any(id, &["h1", "h2", "h3"])
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

            if h_pos.is_some() {
                // Remove metadata elements immediately before the first heading
                if let Some(prev_id) = doc.prev_element_sibling(h_id) {
                    let class = doc.get_attr(prev_id, "class").unwrap_or("").to_lowercase();
                    if doc.tag_name_is_any(prev_id, &["div", "p", "span"]) {
                        let text = doc.text_content(prev_id).to_lowercase();
                        if class.contains("meta")
                            || class.contains("date")
                            || class.contains("author")
                            || class.contains("byline")
                            || class.contains("metadata")
                            || text.starts_with("published ")
                            || text.starts_with("updated ")
                            || text.contains("by ")
                        {
                            let has_profile_link = doc
                                .query_selector_all_in_subtree(prev_id, "a")
                                .iter()
                                .any(|id| {
                                    doc.get_attr(*id, "href")
                                        .is_some_and(|href| href.contains("/@"))
                                });
                            let has_image =
                                !doc.query_selector_all_in_subtree(prev_id, "img").is_empty();
                            if !(has_profile_link && has_image) {
                                doc.remove(prev_id);
                            }
                        }
                    }
                }

                // Remove metadata elements immediately after the first heading
                if let Some(next_id) = doc.next_element_sibling(h_id) {
                    let class = doc.get_attr(next_id, "class").unwrap_or("").to_lowercase();
                    if doc.tag_name_is_any(next_id, &["div", "p", "span"]) {
                        let text = doc.text_content(next_id).to_lowercase();
                        let has_child_date = doc
                            .query_selector_all_in_subtree(next_id, "p,time")
                            .iter()
                            .any(|child| {
                                crate::metadata::parse_date_text(&doc.text_content(*child))
                                    .is_some()
                            });
                        if class.contains("frontmatter") {
                            doc.remove(next_id);
                        } else if class.contains("meta")
                            || class.contains("date")
                            || class.contains("author")
                            || class.contains("byline")
                            || class.contains("metadata")
                            || class.contains("subtitle")
                            || class.contains("published-at")
                            || class.contains("posted-on")
                            || class.contains("header")
                            || (text.starts_with("by ") && !text.contains(['.', '!', '?']))
                            || (text.starts_with("posted ") && !text.contains(['.', '!', '?']))
                            || (text.starts_with("published ") && !text.contains(['.', '!', '?']))
                            || (text.starts_with("updated ") && !text.contains(['.', '!', '?']))
                            || text.contains("min read")
                            || text.contains("minute read")
                            || has_child_date
                        {
                            let word_count = text.split_whitespace().count();
                            if word_count < 30
                                && doc.query_selector_all_in_subtree(next_id, "img").is_empty()
                            {
                                doc.remove(next_id);
                            }
                        }
                        // Also check for date patterns in text content
                        let date_check_re = DATE_CHECK_RE.get_or_init(|| {
                            regex_lite::Regex::new(
                                r"(?:(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},\s*\d{4})"
                            ).unwrap()
                        });
                        if date_check_re.is_match(&text) {
                            let word_count = text.split_whitespace().count();
                            if word_count < 20
                                && doc.query_selector_all_in_subtree(next_id, "img").is_empty()
                            {
                                doc.remove(next_id);
                            }
                        }
                    }
                }
            }
        }

        // A title is often nested in a wrapper while the author/date block is
        // its sibling (for example, a clickable Webflow author card). Walk out
        // through title-only wrappers and remove that adjacent metadata block.
        let mut wrapper = h_id;
        while let Some(parent) = doc.parent(wrapper) {
            if parent == content_id {
                break;
            }
            let siblings = doc.children(parent).to_vec();
            if siblings
                .iter()
                .any(|&id| id != wrapper && !doc.text_content(id).trim().is_empty())
            {
                break;
            }
            wrapper = parent;
        }
        if let Some(_parent) = doc.parent(wrapper)
            && let Some(next) = doc.next_element_sibling(wrapper)
        {
            let class = doc.get_attr(next, "class").unwrap_or("").to_lowercase();
            let text = doc.text_content(next).to_lowercase();
            if class.contains("author")
                || class.contains("byline")
                || class.contains("meta")
                || class.contains("post-date")
                || (text.split_whitespace().count() < 30
                    && (text.starts_with("published ")
                        || text.starts_with("updated ")
                        || text.contains("march")
                        || text.contains("january")
                        || text.contains("february")
                        || text.contains("april")))
            {
                let has_profile_link =
                    doc.query_selector_all_in_subtree(next, "a")
                        .iter()
                        .any(|id| {
                            doc.get_attr(*id, "href")
                                .is_some_and(|href| href.contains("/@"))
                        });
                let has_image = !doc.query_selector_all_in_subtree(next, "img").is_empty();
                if !(has_profile_link && has_image) {
                    doc.remove(next);
                }
            }
        }
    }
}

pub fn remove_author_sibling(doc: &mut Document, content_id: NodeId, author: &str) {
    if author.trim().is_empty() || author.split_whitespace().count() < 2 {
        return;
    }
    let Some(heading) = doc
        .query_selector_all_in_subtree(content_id, "h1,h2")
        .into_iter()
        .next()
    else {
        return;
    };
    let Some(sibling) = doc.next_element_sibling(heading) else {
        return;
    };
    let text = doc.text_content(sibling);
    if text.contains(author)
        && text.split_whitespace().count() < 40
        && !text.contains(['.', '!', '?'])
        && doc.query_selector_all_in_subtree(sibling, "img").is_empty()
    {
        doc.remove(sibling);
    }
}

pub fn count_html_words(content: &str) -> usize {
    let mut text = String::with_capacity(content.len());
    let mut offset = 0;
    while offset < content.len() {
        let remaining = &content[offset..];
        let entity = if remaining.starts_with("&nbsp;") {
            Some((6, ' '))
        } else if remaining.starts_with("&amp;") {
            Some((5, '&'))
        } else if remaining.starts_with("&lt;") {
            Some((4, '<'))
        } else if remaining.starts_with("&gt;") {
            Some((4, '>'))
        } else if remaining.starts_with("&quot;") {
            Some((6, '"'))
        } else {
            None
        };

        if let Some((length, character)) = entity {
            text.push(character);
            offset += length;
            continue;
        }

        let character = remaining
            .chars()
            .next()
            .expect("offset stays on a char boundary");
        offset += character.len_utf8();
        if character == '<' || character == '>' {
            text.push(' ');
        } else if !character.is_ascii_digit() || character == ' ' {
            text.push(character);
        }
    }

    count_words(&text)
}

// Content-pattern helper functions.

fn related_heading_pattern(text: &str) -> bool {
    let re = RELATED_HEADING_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)^(?:related (?:posts?|articles?|content|stories|reads?|reading)|you (?:might|may|could) (?:also )?(?:like|enjoy|be interested in)|read (?:next|more|also)|further reading|see also|more (?:from .*|from|articles?|posts?|like this)|more to (?:read|explore)|explore more|about (?:the )?author|latest (?:news|events?|posts?|articles?|stories))(?:[\s&+]+(?:news|events?|posts?|articles?|stories))*$",
        )
        .unwrap()
    });
    re.is_match(text.trim())
}

fn is_cta_heading(text: &str) -> bool {
    let re = CTA_HEADING_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)^(?:subscribe|sign up|follow us|share this|stay (?:updated|connected)|join (?:us|our)|search (?:the |our )?(?:site|blog|archives?|newsroom))",
        )
        .unwrap()
    });
    re.is_match(text.trim())
}

fn is_appendix_heading(text: &str) -> bool {
    let re = APPENDIX_HEADING_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)^(?:appendix|acknowledg?ments|disclaimers?|dedication|colophon|epilogue|further reading|references?|bibliography|notes|footnotes)$",
        )
        .unwrap()
    });
    re.is_match(text.trim())
}

/// Remove author+date byline blocks that appear near the top of article content.
/// These typically contain an author link and a date within a short wrapper element.
fn remove_byline_blocks(doc: &mut Document, content_id: NodeId) {
    for section in doc.query_selector_all_in_subtree(content_id, "section") {
        let text = doc.text_content(section);
        if text.split_whitespace().count() < 15
            && text.contains("By ")
            && doc.count_selector_in_subtree(section, "img") == 1
        {
            doc.remove(section);
        }
    }
    let children = doc.children(content_id).to_vec();
    // Only check the first few children of the content container — bylines are at the top
    for &child in children.iter().take(6) {
        if !doc.is_element(child) {
            continue;
        }
        if !doc.tag_name_is_any(child, &["div", "p", "span"]) {
            continue;
        }
        let class = doc.get_attr(child, "class").unwrap_or("").to_lowercase();
        let text = doc.text_content(child).trim().to_string();
        let word_count = text.split_whitespace().count();

        if class.split_whitespace().any(|name| name == "author-bio") {
            continue;
        }

        if doc.tag_name_is(child, "section")
            && word_count < 15
            && text.contains("By ")
            && crate::metadata::parse_date_text(&text).is_some()
            && doc.count_selector_in_subtree(child, "img") == 1
        {
            doc.remove(child);
            continue;
        }

        if (class.contains("byline") || class.contains("author") || class.contains("meta"))
            && word_count < 30
        {
            doc.remove(child);
            continue;
        }
        // Also match patterns with author link + date text
        if (2..=20).contains(&word_count) {
            let has_author_link = class.contains("author")
                || class.contains("byline")
                || doc.count_selector_in_subtree(child, "a[href*=\"/author/\"]") == 1;
            let has_date = crate::metadata::parse_date_text(&text).is_some()
                || doc.count_selector_in_subtree(child, "time") == 1;
            if has_author_link && has_date {
                let has_profile_image = doc.has_selector_in_subtree(child, "img")
                    && doc
                        .query_selector_all_in_subtree(child, "a")
                        .iter()
                        .any(|id| {
                            doc.get_attr(*id, "href")
                                .is_some_and(|href| href.contains("/@"))
                        });
                if !has_profile_image {
                    doc.remove(child);
                }
            }
        }
    }
}

/// Remove sections headed by "Related Posts" / "Read Next" etc.
fn remove_related_posts_sections(doc: &mut Document, content_id: NodeId) {
    let headings = doc.query_selector_all_in_subtree(content_id, "h2,h3,h4,h5");
    for &h_id in &headings {
        if doc.is_removed(h_id) {
            continue;
        }
        let text = doc.text_content(h_id).trim().to_string();
        let is_related = related_heading_pattern(&text);
        let is_cta = is_cta_heading(&text);
        if !is_related && !is_cta {
            continue;
        }

        let target = walk_up_isolated(doc, h_id, content_id);

        // A related block injected between article paragraphs must be removed
        // locally. Truncating from it would discard the rest of the article.
        if has_following_prose(doc, target) {
            if target != h_id {
                doc.remove(target);
            }
            continue;
        }

        if target == h_id {
            // A direct related heading may be a real section heading. CTA
            // headings are safe to remove along with their trailing siblings.
            if !is_cta {
                continue;
            }
            remove_trailing_with_cascade(doc, target, content_id);
        } else {
            remove_trailing_with_cascade(doc, target, content_id);
        }
        break;
    }

    // Some readers expose related links as a trailing section without a
    // heading. Remove a short section whose paragraphs are overwhelmingly
    // anchors, while leaving ordinary article paragraphs untouched.
    let sections = doc.query_selector_all_in_subtree(content_id, "section");
    let children = doc.children(content_id).to_vec();
    for section in sections {
        let Some(pos) = children.iter().position(|&id| id == section) else {
            continue;
        };
        if children[pos + 1..].iter().any(|&id| {
            (doc.is_element(id) && !doc.tag_name(id).eq_ignore_ascii_case("hr"))
                && !doc.text_content(id).trim().is_empty()
        }) {
            continue;
        }
        let paragraphs = doc.query_selector_all_in_subtree(section, "p");
        let links = doc.query_selector_all_in_subtree(section, "a");
        let words = doc.text_content(section).split_whitespace().count();
        if paragraphs.len() >= 2
            && links.len() >= 5
            && words < 100
            && doc
                .query_selector_all_in_subtree(section, "h1,h2,h3,h4")
                .is_empty()
        {
            doc.remove(section);
        }
    }
}

fn has_following_prose(doc: &Document, node: NodeId) -> bool {
    let mut sibling = doc.next_element_sibling(node);
    while let Some(id) = sibling {
        if doc.tag_name_is(id, "p") && count_words_in_node(doc, id) >= 25 {
            return true;
        }
        if doc
            .query_selector_all_in_subtree(id, "p")
            .iter()
            .any(|&p| count_words_in_node(doc, p) >= 25)
        {
            return true;
        }
        sibling = doc.next_element_sibling(id);
    }
    false
}

fn count_words_in_node(doc: &Document, node: NodeId) -> usize {
    crate::content_utils::count_words(&doc.text_content(node))
}

fn preceding_prose_words(doc: &Document, node: NodeId, content_id: NodeId) -> usize {
    let mut words = 0;
    let mut current = Some(node);
    while let Some(id) = current {
        if id == content_id {
            break;
        }
        let mut sibling = doc.prev_element_sibling(id);
        while let Some(sibling_id) = sibling {
            if doc.tag_name_is(sibling_id, "p") {
                words += count_words_in_node(doc, sibling_id);
            } else {
                for p in doc.query_selector_all_in_subtree(sibling_id, "p") {
                    words += count_words_in_node(doc, p);
                }
            }
            sibling = doc.prev_element_sibling(sibling_id);
        }
        current = doc.parent(id);
    }
    words
}

fn walk_up_isolated(doc: &Document, node: NodeId, content_id: NodeId) -> NodeId {
    let mut target = node;
    while let Some(parent) = doc.parent(target) {
        if parent == content_id {
            break;
        }
        let mut preceding_words = 0;
        let mut sibling = doc.prev_element_sibling(target);
        while let Some(sibling_id) = sibling {
            preceding_words += count_words_in_node(doc, sibling_id);
            if preceding_words > 10 {
                break;
            }
            sibling = doc.prev_element_sibling(sibling_id);
        }
        if preceding_words > 10 {
            break;
        }
        target = parent;
    }
    target
}

fn remove_trailing_with_cascade(doc: &mut Document, target: NodeId, content_id: NodeId) {
    let mut levels = Vec::new();
    let mut current = target;
    while let Some(parent) = doc.parent(current) {
        if parent == content_id {
            break;
        }
        levels.push((current, parent));
        current = parent;
    }

    for (current, parent) in levels {
        let children = doc.children(parent).to_vec();
        if let Some(position) = children.iter().position(|&id| id == current) {
            for sibling in children.into_iter().skip(position + 1) {
                if doc.get_attr(sibling, "id") == Some("footnotes") {
                    continue;
                }
                doc.remove(sibling);
            }
        }
    }
    doc.remove(target);
}

/// Remove table-of-contents: lists where most links are same-page anchors.
fn remove_table_of_contents(doc: &mut Document, content_id: NodeId) {
    let lists = doc.query_selector_all_in_subtree(content_id, "ul,ol");
    for list_id in lists {
        let items = doc.query_selector_all_in_subtree(list_id, "li");
        if items.len() < 3 {
            continue;
        }
        let _list_text = doc.text_content(list_id).trim().to_string();
        // Must be near the top of content — check via DOM position
        let mut ancestor = doc.parent(list_id);
        let mut depth = 0;
        while let Some(a) = ancestor {
            if a == content_id {
                break;
            }
            depth += 1;
            ancestor = doc.parent(a);
        }
        if depth > 3 {
            continue;
        }
        // Count anchor links
        let links: Vec<_> = doc.query_selector_all_in_subtree(list_id, "a");
        if links.len() < 3 {
            continue;
        }
        let anchor_count = links
            .iter()
            .filter(|&&id| doc.get_attr(id, "href").is_some_and(|h| h.starts_with('#')))
            .count();
        if anchor_count < 3 || (anchor_count as f64) / (links.len() as f64) < 0.8 {
            continue;
        }
        // Remove surrounding HRs
        if let Some(pid) = doc.parent(list_id) {
            let siblings = doc.children(pid).to_vec();
            let pos = siblings.iter().position(|&s| s == list_id);
            if let Some(p) = pos {
                let previous = siblings[..p]
                    .iter()
                    .rev()
                    .find(|&&id| !doc.is_text(id) || !doc.text_content(id).trim().is_empty());
                let next = siblings[p + 1..]
                    .iter()
                    .find(|&&id| !doc.is_text(id) || !doc.text_content(id).trim().is_empty());
                if previous.is_some_and(|&id| {
                    doc.is_element(id) && doc.tag_name(id).eq_ignore_ascii_case("hr")
                }) {
                    doc.remove(*previous.unwrap());
                }
                if next.is_some_and(|&id| {
                    doc.is_element(id) && doc.tag_name(id).eq_ignore_ascii_case("hr")
                }) {
                    doc.remove(*next.unwrap());
                }
            }
        }
        doc.remove(list_id);
        break;
    }
}

/// Remove newsletter/subscribe call-to-action blocks.
fn remove_newsletter_blocks(doc: &mut Document, content_id: NodeId) {
    let newsletter_re = NEWSLETTER_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)\bsubscribe\b[\s\S]{0,40}\bnewsletter\b|\bnewsletter\b[\s\S]{0,40}\bsubscribe\b|\bsign[- ]up\b[\s\S]{0,80}\b(?:newsletter|email alert)|\b(?:don['\u2019]?t (?:want to )?miss|never miss)\b"
        ).unwrap()
    });
    let els = doc.query_selector_all_in_subtree(content_id, "div,p,section,aside,span");
    let mut text = String::new();
    for id in els {
        if doc.has_closest(id, &["pre", "code"]) {
            continue;
        }
        text.clear();
        doc.text_content_into(id, &mut text);
        let text = text.trim();
        let word_count = text.split_whitespace().count();
        if !(2..=50).contains(&word_count) {
            continue;
        }
        if newsletter_re.is_match(text) {
            doc.remove(id);
        }
    }
}

/// Remove social engagement counters like "13 Likes", "5 Comments", "2 Shares".
fn remove_social_counters(doc: &mut Document, content_id: NodeId) {
    let social_re = SOCIAL_COUNTER_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)^\d+\s+(?:Likes?|Comments?|Shares?|Retweets?|Reposts?|Restacks?|Views?)$",
        )
        .unwrap()
    });
    let els = doc.query_selector_all_in_subtree(content_id, "div,p,span,a,button");
    let mut text = String::new();
    for id in els {
        if doc.has_closest(id, &["pre", "code"]) {
            continue;
        }
        text.clear();
        doc.text_content_into(id, &mut text);
        let text_value = text.trim();
        if social_re.is_match(text_value)
            && let Some(pid) = doc.parent(id)
        {
            text.clear();
            doc.text_content_into(pid, &mut text);
            let wrapper_text = text.trim();
            if wrapper_text.split_whitespace().count() < 10 {
                doc.remove(pid);
            }
        }
    }
}

/// Remove social counter links: anchors wrapping "Likes" / "Comments" text.
fn remove_social_counter_links(doc: &mut Document, content_id: NodeId) {
    let likes_re = LIKES_RE
        .get_or_init(|| regex_lite::Regex::new(r"(?i)^\d+\s+(?:Likes?|Comments?)$").unwrap());
    let anchors = doc.query_selector_all_in_subtree(content_id, "a");
    let mut text = String::new();
    for a_id in anchors {
        text.clear();
        doc.text_content_into(a_id, &mut text);
        let text_value = text.trim();
        if likes_re.is_match(text_value) {
            let mut target = a_id;
            for _ in 0..3 {
                if let Some(p) = doc.parent(target) {
                    if p == content_id {
                        break;
                    }
                    text.clear();
                    doc.text_content_into(p, &mut text);
                    let pt = text.trim();
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
            if !doc.has_closest(id, &["pre", "code"]) {
                doc.remove(id);
            }
        }
    }
}

/// Remove trailing CTA / appendix sections: thin heading-introduced blocks at end.
fn remove_trailing_cta_sections(doc: &mut Document, content_id: NodeId) {
    if !doc
        .query_selector_all_in_subtree(content_id, ".author-bio")
        .is_empty()
        || doc
            .query_selector_all_in_subtree(content_id, "h1,h2,h3,h4")
            .iter()
            .any(|id| {
                doc.text_content(*id)
                    .trim()
                    .eq_ignore_ascii_case("conclusion")
            })
    {
        return;
    }
    if !doc
        .query_selector_all_in_subtree(content_id, ".author-bio")
        .is_empty()
    {
        return;
    }
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
        if doc
            .get_attr(child, "class")
            .unwrap_or("")
            .split_whitespace()
            .any(|class| class.eq_ignore_ascii_case("author-bio"))
        {
            break;
        }
        let words = doc.text_content(child).split_whitespace().count();
        // HR is a content boundary — include it and stop walking
        if doc.tag_name_is(child, "hr") {
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
        let has_heading = trailing_ids.iter().any(|&id| {
            is_heading_tag(doc, id)
                || !doc
                    .query_selector_all_in_subtree(id, "h1,h2,h3,h4")
                    .is_empty()
        });
        let has_conclusion = trailing_ids.iter().any(|&id| {
            is_heading_tag(doc, id)
                && doc
                    .text_content(id)
                    .trim()
                    .eq_ignore_ascii_case("conclusion")
        });
        let has_cta = trailing_ids.iter().any(|&id| {
            let text = doc.text_content(id).trim().to_string();
            is_cta_heading(&text) || is_appendix_heading(&text)
        });
        let is_link_dense = trailing_ids.iter().any(|&id| {
            let links = doc.query_selector_all_in_subtree(id, "a").len();
            let words = doc.text_content(id).split_whitespace().count();
            links > 0 && (links as f64) > (words as f64) * 0.3
        });
        if !has_conclusion && (has_heading || has_cta || is_link_dense) {
            for id in trailing_ids {
                doc.remove(id);
            }
        }
    }
}

/// Remove related-card grids without headings while preserving mid-article image rows.
fn remove_card_grid_blocks(doc: &mut Document, content_id: NodeId) {
    let content_words = count_words_in_node(doc, content_id);
    let containers = doc.query_selector_all_in_subtree(content_id, "div,ul,ol");
    for grid_id in containers {
        if doc.is_removed(grid_id) {
            continue;
        }
        let children: Vec<NodeId> = doc
            .children(grid_id)
            .iter()
            .copied()
            .filter(|id| doc.is_element(*id))
            .collect();
        if children.len() < 2 {
            continue;
        }

        let card_count = children
            .iter()
            .filter(|&&child| {
                doc.has_selector_in_subtree(child, "img,picture")
                    && doc.has_selector_in_subtree(child, "h2,h3,h4,a[href]")
            })
            .count();
        if card_count < 2 || card_count * 10 < children.len() * 7 {
            continue;
        }

        // A grid should trail substantial article prose. This excludes top-of-
        // page listings and navigation-like card groups.
        if preceding_prose_words(doc, grid_id, content_id) < 100 {
            continue;
        }

        let grid_words = count_words_in_node(doc, grid_id);
        if content_words > 0 && grid_words * 10 > content_words * 3 {
            continue;
        }

        let target = walk_up_isolated(doc, grid_id, content_id);
        if target == grid_id {
            continue;
        }
        // A card list that is a sibling after a complete article is often a
        // separate page-level recommendation region. Leave it available to
        // the generic content scorer; inline grids inside the article body
        // remain eligible for the checks below.
        if doc.parent(target) == Some(content_id)
            && doc
                .prev_element_sibling(target)
                .is_some_and(|sibling| doc.tag_name_is(sibling, "article"))
        {
            continue;
        }
        let target_words = count_words_in_node(doc, target);
        if target_words > grid_words * 2 + 15 {
            continue;
        }
        if has_following_prose(doc, target) {
            continue;
        }

        remove_trailing_with_cascade(doc, target, content_id);
        break;
    }
}

#[cfg(test)]
mod tests {
    use super::{remove_by_selector, remove_content_patterns};
    use crate::dom::Document;

    #[test]
    fn partial_selectors_match_logo_at_token_edges() {
        let mut doc = Document::parse(
            r#"<article>
                <div class="logo">standalone</div>
                <div class="logo-wrapper">prefix</div>
                <div class="site-logo">suffix</div>
                <div class="a-content--logo-end">content</div>
                <div class="logotype">word</div>
                <div class="adcontainer">ad</div>
                <div class="articleheader">header</div>
                <div class="media-card">media</div>
                <div class="navcontainer">navigation</div>
                <div class="ratingscontainer">ratings</div>
                <div class="reactions">reactions</div>
                <div class="sidebar-element">sidebar</div>
                <ignore>ignored</ignore>
            </article>"#,
        );
        let root = doc
            .query_selector_all("article")
            .first()
            .copied()
            .expect("article root");

        remove_by_selector(&mut doc, true, true);

        let html = doc.outer_html(root);
        assert!(!html.contains("standalone"));
        assert!(!html.contains("prefix"));
        assert!(!html.contains("suffix"));
        assert!(html.contains("content"));
        assert!(!html.contains("word"));
        assert!(!html.contains("ad"));
        assert!(!html.contains("header"));
        assert!(!html.contains("media"));
        assert!(!html.contains("navigation"));
        assert!(!html.contains("ratings"));
        assert!(!html.contains("reactions"));
        assert!(!html.contains("sidebar"));
        assert!(!html.contains("ignored"));
    }

    #[test]
    fn removes_inline_related_cards_without_truncating_following_prose() {
        let prose_before = "The article begins with careful historical context and several paragraphs that establish the subject for readers. ".repeat(4);
        let prose_after = "The later discussion continues with evidence, comparisons, and conclusions that belong to the article and must remain available after cleanup. ".repeat(4);
        let html = format!(
            "<article><p>{prose_before}</p><div class=\"injected-story-block\"><h2>Related Stories</h2><div><article><a href=\"/one\"><img src=\"/one.jpg\"></a><h3>One story</h3></article><article><a href=\"/two\"><img src=\"/two.jpg\"></a><h3>Two story</h3></article></div></div><p>{prose_after}</p></article>"
        );
        let mut doc = Document::parse(&html);
        let root = doc
            .query_selector_all("article")
            .first()
            .copied()
            .expect("article root");

        remove_content_patterns(&mut doc, root);

        let output = doc.outer_html(root);
        assert!(!output.contains("Related Stories"));
        assert!(!output.contains("One story"));
        assert!(output.contains("later discussion continues"));
    }

    #[test]
    fn removes_trailing_image_card_grid() {
        let prose = "This article contains substantial reporting and explanation for readers who need the full context before reaching the recommendations at the end. ".repeat(8);
        let html = format!(
            "<article><p>{prose}</p><div class=\"tile-grid\"><ul><li><a href=\"/one\"><img src=\"/one.jpg\"></a></li><li><a href=\"/two\"><img src=\"/two.jpg\"></a></li><li><a href=\"/three\"><img src=\"/three.jpg\"></a></li></ul></div></article>"
        );
        let mut doc = Document::parse(&html);
        let root = doc
            .query_selector_all("article")
            .first()
            .copied()
            .expect("article root");

        remove_content_patterns(&mut doc, root);

        let output = doc.outer_html(root);
        assert!(!output.contains("tile-grid"));
        assert!(output.contains("substantial reporting"));
    }

    #[test]
    fn keeps_mid_article_multi_image_rows() {
        let prose = "The methods section describes a repeatable process with enough detail for comparison across sites and subsequent review. ".repeat(5);
        let html = format!(
            "<article><p>{prose}</p><section><h2>Methods</h2><div class=\"tmulti\"><div class=\"trow\"><div class=\"tsingle\"><a href=\"/one\"><img src=\"/one.jpg\"></a></div><div class=\"tsingle\"><a href=\"/two\"><img src=\"/two.jpg\"></a></div></div></div><p>{prose}</p></section></article>"
        );
        let mut doc = Document::parse(&html);
        let root = doc
            .query_selector_all("article")
            .first()
            .copied()
            .expect("article root");

        remove_content_patterns(&mut doc, root);

        let output = doc.outer_html(root);
        assert!(output.contains("tmulti"));
        assert!(output.contains("Methods"));
        assert!(output.contains("one.jpg"));
        assert!(output.contains("later") || output.contains("methods section"));
    }
}
