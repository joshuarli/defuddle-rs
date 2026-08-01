use crate::content_utils::{contains_ascii_case_insensitive, count_words};
use crate::dom::{Document, NodeId};

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
    let text = doc.text_content(node);
    score_element_with_text(doc, node, &text)
}

pub fn score_element_with_text(doc: &Document, node: NodeId, text: &str) -> f64 {
    let mut score = 0.0;
    let words = count_words(text) as f64;
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

    let class_val = doc.get_attr(node, "class").unwrap_or("");
    if contains_ascii_case_insensitive(class_val, "content")
        || contains_ascii_case_insensitive(class_val, "article")
        || contains_ascii_case_insensitive(class_val, "post")
    {
        score += 15.0;
    }

    let table_count = count_descendant_tag(doc, node, "table") as f64;
    score -= table_count * 5.0;

    let mut link_text_len = 0;
    let mut link_text = String::new();
    for_each_descendant_tag(doc, node, "a", &mut |link_id| {
        link_text.clear();
        doc.text_content_into(link_id, &mut link_text);
        link_text_len += link_text.len();
    });
    let text_len = text.len().max(1) as f64;
    let link_density = f64::min(link_text_len as f64 / text_len, 0.5);
    score *= 1.0 - link_density;

    score
}

fn count_descendant_tag(doc: &Document, node: NodeId, tag: &str) -> usize {
    let mut count = 0;
    for_each_descendant_tag(doc, node, tag, &mut |_| count += 1);
    count
}

fn for_each_descendant_tag(
    doc: &Document,
    node: NodeId,
    tag: &str,
    visit: &mut impl FnMut(NodeId),
) {
    for &child in doc.children(node) {
        if doc.tag_name(child).eq_ignore_ascii_case(tag) {
            visit(child);
        }
        for_each_descendant_tag(doc, child, tag, visit);
    }
}

pub fn score_non_content_block_with_text(doc: &Document, node: NodeId, text: &str) -> f64 {
    let words = count_words(text);
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

    let links = count_descendant_tag(doc, node, "a");
    let link_density = links as f64 / words as f64;
    if link_density > 0.5 {
        score -= 15.0;
    }
    if links > 1 && words < 80 {
        let mut link_text_len = 0;
        let mut link_text = String::new();
        for_each_descendant_tag(doc, node, "a", &mut |link_id| {
            link_text.clear();
            doc.text_content_into(link_id, &mut link_text);
            link_text_len += link_text.len();
        });
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
        let mut social_profile = false;
        for_each_descendant_tag(doc, node, "a", &mut |link_id| {
            if let Some(href) = doc.get_attr(link_id, "href")
                && SOCIAL_PROFILE_PATTERNS
                    .iter()
                    .any(|p| contains_ascii_case_insensitive(href, p))
            {
                social_profile = true;
            }
        });
        if social_profile {
            score -= 15.0;
        }
    }

    let id_str = doc.get_attr(node, "id").unwrap_or("");
    for pattern in NON_CONTENT_PATTERNS {
        if *pattern == "sidebar" && words >= 80 && count_descendant_tag(doc, node, "p") >= 3 {
            continue;
        }
        if contains_ascii_case_insensitive(class_val, pattern)
            || contains_ascii_case_insensitive(id_str, pattern)
        {
            score -= 8.0;
        }
    }

    // Penalize blocks whose heading text matches related-posts patterns
    let mut related_heading = false;
    for tag in ["h1", "h2", "h3", "h4"] {
        let mut heading_text = String::new();
        for_each_descendant_tag(doc, node, tag, &mut |h_id| {
            heading_text.clear();
            doc.text_content_into(h_id, &mut heading_text);
            if contains_ascii_case_insensitive(&heading_text, "related")
                || heading_text
                    .get(..5)
                    .is_some_and(|s| s.eq_ignore_ascii_case("more "))
                || contains_ascii_case_insensitive(&heading_text, "you might")
                || contains_ascii_case_insensitive(&heading_text, "you may")
                || contains_ascii_case_insensitive(&heading_text, "further reading")
                || contains_ascii_case_insensitive(&heading_text, "see also")
                || contains_ascii_case_insensitive(&heading_text, "read next")
                || contains_ascii_case_insensitive(&heading_text, "popular")
                || contains_ascii_case_insensitive(&heading_text, "recommended")
                || contains_ascii_case_insensitive(&heading_text, "trending")
            {
                related_heading = true;
            }
        });
    }
    if related_heading {
        score -= 20.0;
    }

    // Penalize table cells that are mostly links (link farms)
    if doc.tag_name(node).eq_ignore_ascii_case("td")
        || doc.tag_name(node).eq_ignore_ascii_case("th")
    {
        let links = count_descendant_tag(doc, node, "a");
        if links > 0 {
            let mut link_text_len = 0;
            let mut link_text = String::new();
            for_each_descendant_tag(doc, node, "a", &mut |link_id| {
                link_text.clear();
                doc.text_content_into(link_id, &mut link_text);
                link_text_len += link_text.len();
            });
            let total_len = text.len().max(1);
            if (link_text_len as f64 / total_len as f64) > 0.6 {
                score -= 10.0;
            }
        }
    }

    score
}

pub fn find_main_content(doc: &Document, entry_point_selectors: &[&str]) -> Option<NodeId> {
    find_main_content_with_candidates(doc, entry_point_selectors, |selector| {
        doc.query_selector_all(selector)
    })
}

fn find_main_content_with_candidates(
    doc: &Document,
    entry_point_selectors: &[&str],
    mut select: impl FnMut(&str) -> Vec<NodeId>,
) -> Option<NodeId> {
    let mut candidates: Vec<(NodeId, f64, usize)> = Vec::new();

    for (index, selector) in entry_point_selectors.iter().enumerate() {
        let elements = select(selector);
        for el in elements {
            let mut text = String::new();
            doc.text_content_into(el, &mut text);
            let score = (entry_point_selectors.len() - index) as f64 * 40.0
                + score_element_with_text(doc, el, &text);
            candidates.push((el, score, index));
        }
    }

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let top = candidates[0];
    if candidates.len() == 1 {
        if doc.tag_name_is(top.0, "body") {
            return find_table_based_content(doc);
        }
        return Some(top.0);
    }

    let mut best = top;
    let mut child_text = String::new();
    for i in 1..candidates.len() {
        let child = candidates[i];
        doc.text_content_into(child.0, &mut child_text);
        let child_words = count_words(&child_text);
        if child.2 < best.2 && doc.contains(best.0, child.0) && child_words > 50 {
            if doc.tag_name_is(top.0, "body") {
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
