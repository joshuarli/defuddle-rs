#![allow(dead_code)]

use crate::dom::{Document, NodeId};
use crate::utils::count_words;

const CONTENT_INDICATORS: &[&str] = &[
    "admonition",
    "article",
    "content",
    "entry",
    "image",
    "img",
    "font",
    "figure",
    "figcaption",
    "pre",
    "main",
    "post",
    "story",
    "table",
];

const NAVIGATION_INDICATORS: &[&str] = &[
    "advertisement",
    "all rights reserved",
    "banner",
    "cookie",
    "comments",
    "copyright",
    "follow me",
    "follow us",
    "footer",
    "header",
    "homepage",
    "login",
    "menu",
    "more articles",
    "more like this",
    "most read",
    "nav",
    "navigation",
    "newsletter",
    "popular",
    "privacy",
    "recommended",
    "register",
    "related",
    "responses",
    "share",
    "sidebar",
    "sign in",
    "sign up",
    "signup",
    "social",
    "sponsored",
    "subscribe",
    "terms",
    "trending",
];

const NON_CONTENT_PATTERNS: &[&str] = &[
    "advert",
    "ad-",
    "ads",
    "banner",
    "cookie",
    "copyright",
    "footer",
    "header",
    "homepage",
    "menu",
    "nav",
    "newsletter",
    "popular",
    "privacy",
    "recommended",
    "related",
    "rights",
    "share",
    "sidebar",
    "social",
    "sponsored",
    "subscribe",
    "terms",
    "trending",
    "widget",
];

const SOCIAL_PROFILE_PATTERNS: &[&str] = &[
    "linkedin.com/in/",
    "linkedin.com/company/",
    "twitter.com/",
    "x.com/",
    "facebook.com/",
    "instagram.com/",
    "threads.net/",
    "mastodon.",
];

pub fn score_element(doc: &Document, node: NodeId) -> f64 {
    let mut score = 0.0;

    let text = doc.text_content(node);
    let words = count_words(&text) as f64;
    if words == 0.0 {
        return 0.0;
    }

    score += words;

    let p_count = count_descendant_tag(doc, node, "p") as f64;
    score += p_count * 10.0;

    let commas = text.chars().filter(|&c| c == ',').count() as f64;
    score += commas;

    let img_count = count_descendant_tag(doc, node, "img") as f64;
    let image_density = img_count / words;
    score -= image_density * 3.0;

    let class_val = doc.get_attr(node, "class").unwrap_or("").to_lowercase();
    if class_val.contains("content") || class_val.contains("article") || class_val.contains("post")
    {
        score += 15.0;
    }

    let table_count = count_descendant_tag(doc, node, "table") as f64;
    score -= table_count * 5.0;

    let link_ids = collect_descendant_tag(doc, node, "a");
    let mut link_text_len = 0;
    for link_id in &link_ids {
        link_text_len += doc.text_content(*link_id).len();
    }
    let text_len = text.len().max(1) as f64;
    let link_density = f64::min(link_text_len as f64 / text_len, 0.5);
    score *= 1.0 - link_density;

    score
}

fn count_descendant_tag(doc: &Document, node: NodeId, tag: &str) -> usize {
    collect_descendant_tag(doc, node, tag).len()
}

fn collect_descendant_tag(doc: &Document, node: NodeId, tag: &str) -> Vec<NodeId> {
    let mut result = Vec::new();
    collect_descendant_tag_into(doc, node, tag, &mut result);
    result
}

fn collect_descendant_tag_into(doc: &Document, node: NodeId, tag: &str, result: &mut Vec<NodeId>) {
    for child in doc.children(node).to_vec() {
        if doc.tag_name(child).eq_ignore_ascii_case(tag) {
            result.push(child);
        }
        collect_descendant_tag_into(doc, child, tag, result);
    }
}

pub fn is_likely_content(doc: &Document, node: NodeId) -> bool {
    let role = doc.get_attr(node, "role").unwrap_or("").to_lowercase();
    if role == "article" || role == "main" {
        return true;
    }

    let class_val = doc.get_attr(node, "class").unwrap_or("").to_lowercase();
    let id_str = doc.get_attr(node, "id").unwrap_or("").to_lowercase();

    for indicator in CONTENT_INDICATORS {
        if class_val.contains(indicator) || id_str.contains(indicator) {
            return true;
        }
    }

    let has_rich = count_descendant_tag(doc, node, "pre") > 0
        || count_descendant_tag(doc, node, "table") > 0
        || count_descendant_tag(doc, node, "figure") > 0
        || count_descendant_tag(doc, node, "picture") > 0;
    if has_rich {
        return true;
    }

    let text = doc.text_content(node);
    let words = count_words(&text);

    let heading_children: Vec<NodeId> = doc
        .children(node)
        .iter()
        .copied()
        .filter(|&c| {
            let t = doc.tag_name(c).to_lowercase();
            t == "h1" || t == "h2" || t == "h3" || t == "h4" || t == "h5" || t == "h6"
        })
        .collect();

    if let Some(heading) = heading_children.first() {
        let heading_text = doc.text_content(*heading).trim().to_string();
        if heading_text == text.trim() {
            let heading_lower = heading_text.to_lowercase();
            let is_nav_heading = NAVIGATION_INDICATORS
                .iter()
                .any(|ind| heading_lower.contains(ind));
            let is_toc = heading_lower == "contents"
                || heading_lower == "table of contents"
                || heading_lower.contains("on this page");
            if !is_nav_heading && !is_toc {
                return true;
            }
        }
    }

    let paragraphs = count_descendant_tag(doc, node, "p");
    let list_items = count_descendant_tag(doc, node, "li");
    let content_blocks = paragraphs + list_items;

    if words > 50 && content_blocks > 1 {
        return true;
    }
    if words > 100 {
        return true;
    }
    if words > 30 && content_blocks > 0 {
        return true;
    }
    if words >= 10
        && ["?", "!", "."].iter().any(|&p| text.contains(p))
        && (count_descendant_tag(doc, node, "a") as f64) / (words as f64) < 0.1
    {
        return true;
    }

    false
}

pub fn score_non_content_block(doc: &Document, node: NodeId) -> f64 {
    let text = doc.text_content(node);
    score_non_content_block_with_text(doc, node, &text)
}

pub fn score_non_content_block_with_text(doc: &Document, node: NodeId, text: &str) -> f64 {
    let words = count_words(&text);
    if words < 3 {
        return 0.0;
    }
    let class_val = doc.get_attr(node, "class").unwrap_or("");
    if words >= 80
        && (contains_ascii_case_insensitive(class_val, "content")
            || contains_ascii_case_insensitive(class_val, "article")
            || contains_ascii_case_insensitive(class_val, "post"))
        && count_descendant_tag(doc, node, "p") >= 3
    {
        return 0.0;
    }

    let mut score = 0.0;

    let commas = text.chars().filter(|&c| c == ',').count() as f64;
    score += commas;

    let mut indicator_matches = 0;
    for indicator in NAVIGATION_INDICATORS {
        if *indicator == "sidebar" && words >= 80 {
            continue;
        }
        if contains_ascii_case_insensitive(text, indicator) {
            indicator_matches += 1;
        }
    }
    score -= (indicator_matches as f64) * 10.0;

    let link_ids = collect_descendant_tag(doc, node, "a");
    let links = link_ids.len();
    let link_density = links as f64 / words as f64;
    if link_density > 0.5 {
        score -= 15.0;
    }
    if links > 1 && words < 80 {
        let mut link_text_len = 0;
        for link_id in &link_ids {
            link_text_len += doc.text_content(*link_id).len();
        }
        let total_len = text.len();
        if total_len > 0 && (link_text_len as f64 / total_len as f64) > 0.8 {
            score -= 15.0;
        }
    }

    let ul_count = count_descendant_tag(doc, node, "ul");
    let ol_count = count_descendant_tag(doc, node, "ol");
    let lists = ul_count + ol_count;
    if lists > 0 && links > lists * 3 {
        score -= 10.0;
    }

    if words < 80 {
        for link_id in &link_ids {
            if let Some(href) = doc.get_attr(*link_id, "href") {
                let href_lower = href.to_lowercase();
                if SOCIAL_PROFILE_PATTERNS
                    .iter()
                    .any(|p| href_lower.contains(p))
                {
                    score -= 15.0;
                    break;
                }
            }
        }
    }

    let id_str = doc.get_attr(node, "id").unwrap_or("").to_lowercase();
    for pattern in NON_CONTENT_PATTERNS {
        if *pattern == "sidebar" && words >= 80 && count_descendant_tag(doc, node, "p") >= 3 {
            continue;
        }
        if class_val.contains(pattern) || id_str.contains(pattern) {
            score -= 8.0;
        }
    }

    // Penalize blocks whose heading text matches related-posts patterns
    let headings = doc.query_selector_all_in_subtree(node, "h1,h2,h3,h4");
    for h_id in headings {
        let ht = doc.text_content(h_id).to_lowercase();
        if ht.contains("related")
            || ht.starts_with("more ")
            || ht.contains("you might")
            || ht.contains("you may")
            || ht.contains("further reading")
            || ht.contains("see also")
            || ht.contains("read next")
            || ht.contains("popular")
            || ht.contains("recommended")
            || ht.contains("trending")
        {
            score -= 20.0;
            break;
        }
    }

    // Penalize table cells that are mostly links (link farms)
    if doc.tag_name(node).eq_ignore_ascii_case("td")
        || doc.tag_name(node).eq_ignore_ascii_case("th")
    {
        let links = count_descendant_tag(doc, node, "a");
        if links > 0 {
            let mut link_text_len = 0;
            for link_id in &collect_descendant_tag(doc, node, "a") {
                link_text_len += doc.text_content(*link_id).len();
            }
            let total_len = doc.text_content(node).len().max(1);
            if (link_text_len as f64 / total_len as f64) > 0.6 {
                score -= 10.0;
            }
        }
    }

    score
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack.char_indices().any(|(index, _)| {
        haystack[index..]
            .get(..needle.len())
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(needle))
    })
}

pub fn find_main_content(doc: &Document, entry_point_selectors: &[&str]) -> Option<NodeId> {
    let mut candidates: Vec<(NodeId, f64, usize)> = Vec::new();

    for (index, selector) in entry_point_selectors.iter().enumerate() {
        let elements = doc.query_selector_all(selector);
        for el in elements {
            let score =
                (entry_point_selectors.len() - index) as f64 * 40.0 + score_element(doc, el);
            candidates.push((el, score, index));
        }
    }

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let top = candidates[0];
    if candidates.len() == 1 {
        let tag = doc.tag_name(top.0).to_lowercase();
        if tag == "body" {
            return find_table_based_content(doc);
        }
        return Some(top.0);
    }

    let mut best = top;
    for i in 1..candidates.len() {
        let child = candidates[i];
        let child_words = count_words(&doc.text_content(child.0));
        if child.2 < best.2 && doc.contains(best.0, child.0) && child_words > 50 {
            let tag = doc.tag_name(top.0).to_lowercase();
            if tag == "body" {
                continue;
            }
            let mut siblings_at_index = 0;
            for c in &candidates {
                if c.2 == child.2 && doc.contains(top.0, c.0) {
                    siblings_at_index += 1;
                    if siblings_at_index > 1 {
                        break;
                    }
                }
            }
            if siblings_at_index <= 1 {
                best = child;
            }
        }
    }

    if best.0 != top.0 {
        return Some(best.0);
    }
    Some(top.0)
}

fn find_table_based_content(doc: &Document) -> Option<NodeId> {
    let tables = doc.query_selector_all("table");
    let has_table_layout = tables.iter().any(|table_id| {
        let width = doc
            .get_attr(*table_id, "width")
            .and_then(|w| w.parse::<f64>().ok())
            .unwrap_or(0.0);
        let class = doc
            .get_attr(*table_id, "class")
            .unwrap_or("")
            .to_lowercase();
        let align = doc.get_attr(*table_id, "align").unwrap_or("");
        width > 400.0 || align == "center" || class.contains("content") || class.contains("article")
    });

    if !has_table_layout {
        return None;
    }

    let tds = doc.query_selector_all("td");
    if tds.is_empty() {
        return None;
    }

    let mut best_td = None;
    let mut best_score = f64::NEG_INFINITY;
    for td in &tds {
        let s = score_element(doc, *td);
        if s > best_score {
            best_score = s;
            best_td = Some(*td);
        }
    }

    best_td
}
