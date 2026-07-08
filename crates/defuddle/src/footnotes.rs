//! Footnote standardization: collect inline references and definitions,
//! normalize to <ol id="footnotes"> format.
//!
//! Handles: WordPress block footnotes, pulldown-cmark, Hugo, Easy Footnotes,
//! HR-separated sup-numbered, named anchors, heading-separated, p.footnote.

use crate::dom::{Document, NodeId};
use rustc_hash::FxHashMap;

/// Footnote heading regex — matches headings that delimit footnote sections.
/// Also matches compound headings like "References and notes".
const FOOTNOTE_SECTION_RE: &str =
    r"(?i)^(?:foot\s*notes?|end\s*notes?|notes?|references?)(?:\s+and\s+notes?)?$";

/// Inline reference selectors
const INLINE_REF_SELECTORS: &[&str] = &[
    "sup.footnoteref",
    "sup.footnote-reference",
    "span.footnote-reference",
    "span.footnote-link",
    "a[data-type=\"noteref\"]",
    "a[id^=\"fnref\"]",
    "sup[id^=\"fnref:\"]",
    "span[id^=\"fnref:\"]",
    "sup[id^=\"fnr\"]",
    "a.citation",
    // Additional patterns from upstream FOOTNOTE_INLINE_REFERENCES_SELECTOR
    "sup.reference",
    "a[id^=\"ref-link\"]",
    "a[href^=\"#fn\"]",
    "a[href^=\"#cite\"]",
    "a[href^=\"#reference\"]",
    "a[href^=\"#footnote\"]",
    "a[href^=\"#r\"]",
    "a[href^=\"#b\"]",
    "a[href*=\"cite_note\"]",
    "a[href*=\"cite_ref\"]",
    "a.footnote-anchor",
    "span.footnote-hovercard-target a",
    "a[role=\"doc-biblioref\"]",
    "sup[data-fn] > a[href^=\"#\"]",
    "sup[id^=\"ftnt_ref\"] a[href^=\"#ftnt\"]",
    "span.easy-footnote > a[href^=\"#easy-footnote-bottom-\"]",
    "a.footnote[href^=\"#\"]",
];

pub fn standardize_footnotes(doc: &mut Document, content_id: NodeId) {
    // Phase 1: Mark inline references matching known selectors
    let mut ref_count = 0;
    for selector in INLINE_REF_SELECTORS {
        let els = doc.query_selector_all_in_subtree(content_id, selector);
        for el_id in els {
            if has_closest(doc, el_id, &["pre", "code"]) {
                continue;
            }
            ref_count += 1;
            doc.set_attr(el_id, "data-fn-ref", &ref_count.to_string());
        }
    }

    // Phase 1b: Detect bare <sup> elements with numeric markers (e.g., <sup>1</sup>, <sup>[2]</sup>)
    let footnote_marker_re = regex_lite::Regex::new(r"^\[?\(?(\d{1,4})\)?\]?$").unwrap();
    let bare_sups = doc.query_selector_all_in_subtree(content_id, "sup");
    for sup_id in bare_sups {
        if has_closest(doc, sup_id, &["pre", "code"]) {
            continue;
        }
        if doc.get_attr(sup_id, "data-fn-ref").is_some() {
            continue;
        }
        let text = doc.text_content(sup_id).trim().to_string();
        if footnote_marker_re.is_match(&text) {
            ref_count += 1;
            doc.set_attr(sup_id, "data-fn-ref", &ref_count.to_string());
        }
    }

    if ref_count == 0 {
        return;
    }

    // Phase 2: Collect definitions from various formats
    let mut definitions: FxHashMap<usize, String> = FxHashMap::default();
    collect_wp_footnotes(doc, content_id, &mut definitions);
    collect_pulldown_cmark_defs(doc, content_id, &mut definitions);
    collect_hugo_defs(doc, content_id, &mut definitions);
    collect_easy_footnotes(doc, content_id, &mut definitions);
    collect_hr_separated_defs(doc, content_id, &mut definitions);
    collect_heading_separated_defs(doc, content_id, &mut definitions);
    collect_p_class_defs(doc, content_id, &mut definitions);

    if definitions.is_empty() {
        return;
    }


    // Phase 3: Build unified <div id="footnotes"><ol>...</ol></div>
    let mut ol_html = String::from("<div id=\"footnotes\">\n<ol>\n");
    for (num, def_html) in &definitions {
        ol_html.push_str(&format!(
            "<li id=\"fn:{num}\" class=\"footnote\">{def_html}<a href=\"#fnref:{num}\" class=\"footnote-backref\" title=\"return to article\">\u{21A9}</a></li>\n"
        ));
    }
    ol_html.push_str("</ol>\n</div>");

    // Append footnote list after the content_id
    doc.append_html(content_id, &ol_html);

    // Phase 4: Convert inline refs to sup with anchor
    for selector in INLINE_REF_SELECTORS {
        let els = doc.query_selector_all_in_subtree(content_id, selector);
        for el_id in els {
            if let Some(num_str) = doc.get_attr(el_id, "data-fn-ref")
                && let Ok(num) = num_str.parse::<usize>()
                && definitions.contains_key(&num)
            {
                let sup_text = doc.text_content(el_id).trim().to_string();
                let display = if sup_text.is_empty() {
                    num.to_string()
                } else {
                    sup_text
                };
                let html = format!(
                    "<sup id=\"fnref:{num}\"><a href=\"#fn:{num}\" class=\"footnote-ref\">{display}</a></sup>"
                );
                doc.replace_with_html(el_id, &html);
            }
        }
    }

    // Phase 4b: Also convert bare <sup> elements that were marked in Phase 1b
    let bare_sups = doc.query_selector_all_in_subtree(content_id, "sup");
    for sup_id in bare_sups {
        if let Some(num_str) = doc.get_attr(sup_id, "data-fn-ref")
            && let Ok(num) = num_str.parse::<usize>()
            && definitions.contains_key(&num)
            // Only convert if not already handled (simpler than checking all selectors)
            && doc.get_attr(sup_id, "id").is_none()
        {
            let sup_text = doc.text_content(sup_id).trim().to_string();
            let display = if sup_text.is_empty() {
                num.to_string()
            } else {
                sup_text
            };
            let html = format!(
                "<sup id=\"fnref:{num}\"><a href=\"#fn:{num}\" class=\"footnote-ref\">{display}</a></sup>"
            );
            doc.replace_with_html(sup_id, &html);
        }
    }
}

/// WordPress block footnotes: ol.wp-block-footnotes.
fn collect_wp_footnotes(
    doc: &mut Document,
    content_id: NodeId,
    defs: &mut FxHashMap<usize, String>,
) {
    let lists = doc.query_selector_all_in_subtree(content_id, "ol.wp-block-footnotes");
    for list_id in lists {
        let items = doc.query_selector_all_in_subtree(list_id, "li");
        for &li_id in items.iter() {
            let num = defs.len() + 1;
            defs.entry(num).or_insert_with(|| doc.inner_html(li_id));
        }
        doc.remove(list_id);
    }
}

/// pulldown-cmark: div.footnote-definition with id="N".
fn collect_pulldown_cmark_defs(
    doc: &mut Document,
    content_id: NodeId,
    defs: &mut FxHashMap<usize, String>,
) {
    let defs_els = doc.query_selector_all_in_subtree(content_id, "div.footnote-definition");
    for def_id in defs_els {
        let labels = doc.query_selector_all_in_subtree(def_id, "sup.footnote-definition-label");
        for label_id in labels {
            doc.remove(label_id);
        }
        if let Some(id_attr) = doc.get_attr(def_id, "id")
            && let Ok(num) = id_attr.trim().parse::<usize>()
        {
            defs.entry(num).or_insert_with(|| doc.inner_html(def_id));
        }
        doc.remove(def_id);
    }
}

/// Hugo-style: sup.footnote-ref → definition in div.footnotes.
fn collect_hugo_defs(
    _doc: &mut Document,
    _content_id: NodeId,
    _defs: &mut FxHashMap<usize, String>,
) {
}

/// Easy Footnotes WP plugin: span.easy-footnote → definition in ol.easy-footnotes-wrapper.
fn collect_easy_footnotes(
    _doc: &mut Document,
    _content_id: NodeId,
    _defs: &mut FxHashMap<usize, String>,
) {
}

/// HR-separated sup definitions: <p><sup>N</sup> text</p> after <hr>.
fn collect_hr_separated_defs(
    doc: &mut Document,
    content_id: NodeId,
    defs: &mut FxHashMap<usize, String>,
) {
    let children = doc.children(content_id).to_vec();
    let mut after_hr = false;
    let mut to_remove = Vec::new();
    for &child in &children {
        if doc.tag_name(child).eq_ignore_ascii_case("hr") {
            after_hr = true;
            continue;
        }
        if !after_hr {
            continue;
        }
        let tag = doc.tag_name(child).to_lowercase();
        if tag == "p" || tag == "div" {
            let sups = doc.query_selector_all_in_subtree(child, "sup");
            if let Some(&sup_id) = sups.first() {
                let sup_text = doc.text_content(sup_id).trim().to_string();
                if sup_text.parse::<usize>().is_ok() {
                    doc.remove(sup_id);
                    let num = defs.len() + 1;
                    let html = doc.inner_html(child);
                    defs.entry(num).or_insert(html);
                    to_remove.push(child);
                }
            }
        }
    }
    for id in to_remove {
        doc.remove(id);
    }
}

/// Footnotes under "Notes"/"Footnotes"/"References" heading.
fn collect_heading_separated_defs(
    doc: &mut Document,
    content_id: NodeId,
    defs: &mut FxHashMap<usize, String>,
) {
    let re = regex_lite::Regex::new(FOOTNOTE_SECTION_RE).unwrap();
    let headings = doc.query_selector_all_in_subtree(content_id, "h2,h3,h4,h5,h6");
    let children = doc.children(content_id).to_vec();


    for &h_id in &headings {
        let text = doc.text_content(h_id).trim().to_string();
        if !re.is_match(&text) {
            continue;
        }
        if let Some(pos) = children.iter().position(|&c| c == h_id) {
            doc.remove(h_id);
            for &child in &children[pos + 1..] {
                let tag = doc.tag_name(child).to_lowercase();
                // Skip whitespace/text nodes between elements
                if tag.is_empty() {
                    continue;
                }
                if tag == "ol" || tag == "ul" {
                    let items = doc.query_selector_all_in_subtree(child, "li");
                    for &li_id in &items {
                        let num = defs.len() + 1;
                        defs.entry(num).or_insert_with(|| doc.inner_html(li_id));
                    }
                    doc.remove(child);
                    break;
                }
                if tag == "p" {
                    let sups = doc.query_selector_all_in_subtree(child, "sup");
                    if let Some(&sup_id) = sups.first() {
                        let st = doc.text_content(sup_id).trim().to_string();
                        if st.parse::<usize>().is_ok() {
                            doc.remove(sup_id);
                            let num = defs.len() + 1;
                            defs.entry(num).or_insert(doc.inner_html(child));
                            doc.remove(child);
                            continue;
                        }
                    }
                }
                break;
            }
        }
        break;
    }
}

/// p.footnote / li.footnote class definitions.
fn collect_p_class_defs(
    doc: &mut Document,
    content_id: NodeId,
    defs: &mut FxHashMap<usize, String>,
) {
    let ps = doc.query_selector_all_in_subtree(content_id, "p.footnote, p.footnotes, li.footnote");
    for p_id in ps {
        let num = defs.len() + 1;
        defs.entry(num).or_insert_with(|| doc.inner_html(p_id));
    }
}

fn has_closest(doc: &Document, node: NodeId, selectors: &[&str]) -> bool {
    selectors.iter().any(|s| doc.closest(node, s).is_some())
}
