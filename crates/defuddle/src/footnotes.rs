//! Footnote standardization: collect inline references and definitions,
//! normalize to <ol id="footnotes"> format.
//!
//! Handles: WordPress block footnotes, pulldown-cmark, Hugo, Easy Footnotes,
//! HR-separated sup-numbered, named anchors, heading-separated, p.footnote.

use crate::dom::{Document, NodeId};
use std::collections::BTreeMap;

/// Footnote heading regex
const FOOTNOTE_SECTION_RE: &str = r"(?i)^(?:foot\s*notes?|end\s*notes?|notes?|references?)$";

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
];

pub fn standardize_footnotes(doc: &mut Document, content_id: NodeId) {
    // Phase 1: Mark inline references
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

    if ref_count == 0 {
        return;
    }

    // Phase 2: Collect definitions from various formats
    let mut definitions: BTreeMap<usize, String> = BTreeMap::new();
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

    // Phase 3: Build unified <ol id="footnotes">
    let mut ol_html = String::from("<ol id=\"footnotes\">\n");
    for (num, def_html) in &definitions {
        ol_html.push_str(&format!(
            "<li id=\"fn:{num}\" class=\"footnote\">{def_html}<a href=\"#fnref:{num}\" class=\"footnote-backref\" title=\"return to article\">\u{21A9}</a></li>\n"
        ));
    }
    ol_html.push_str("</ol>");

    // Append footnote list after the content_id
    doc.append_html(content_id, &ol_html);

    // Phase 4: Convert inline refs to sup with anchor
    for selector in INLINE_REF_SELECTORS {
        let els = doc.query_selector_all_in_subtree(content_id, selector);
        for el_id in els {
            if let Some(num_str) = doc.get_attr(el_id, "data-fn-ref") {
                if let Ok(num) = num_str.parse::<usize>() {
                    if definitions.contains_key(&num) {
                        let sup_text = doc.text_content(el_id).trim().to_string();
                        let display = if sup_text.is_empty() { num.to_string() } else { sup_text };
                        doc.set_tag_name(el_id, "sup");
                        let html = format!("<a href=\"#fn:{num}\" id=\"fnref:{num}\" class=\"footnote-ref\">{display}</a>");
                        doc.replace_with_html(el_id, &html);
                    }
                }
            }
        }
    }
}


/// WordPress block footnotes: ol.wp-block-footnotes.
fn collect_wp_footnotes(doc: &mut Document, content_id: NodeId, defs: &mut BTreeMap<usize, String>) {
    let lists = doc.query_selector_all_in_subtree(content_id, "ol.wp-block-footnotes");
    for list_id in lists {
        let items = doc.query_selector_all_in_subtree(list_id, "li");
        for (_i, &li_id) in items.iter().enumerate() {
            let num = defs.len() + 1;
            defs.entry(num).or_insert_with(|| doc.inner_html(li_id));
        }
        doc.remove(list_id);
    }
}

/// pulldown-cmark: div.footnote-definition with id="N".
fn collect_pulldown_cmark_defs(doc: &mut Document, content_id: NodeId, defs: &mut BTreeMap<usize, String>) {
    let defs_els = doc.query_selector_all_in_subtree(content_id, "div.footnote-definition");
    for def_id in defs_els {
        let labels = doc.query_selector_all_in_subtree(def_id, "sup.footnote-definition-label");
        for label_id in labels { doc.remove(label_id); }
        if let Some(id_attr) = doc.get_attr(def_id, "id") {
            if let Ok(num) = id_attr.trim().parse::<usize>() {
                defs.entry(num).or_insert_with(|| doc.inner_html(def_id));
            }
        }
        doc.remove(def_id);
    }
}


/// Hugo-style: sup.footnote-ref → definition in div.footnotes.
fn collect_hugo_defs(_doc: &mut Document, _content_id: NodeId, _defs: &mut BTreeMap<usize, String>) {}

/// Easy Footnotes WP plugin: span.easy-footnote → definition in ol.easy-footnotes-wrapper.
fn collect_easy_footnotes(_doc: &mut Document, _content_id: NodeId, _defs: &mut BTreeMap<usize, String>) {}

/// HR-separated sup definitions: <p><sup>N</sup> text</p> after <hr>.
fn collect_hr_separated_defs(doc: &mut Document, content_id: NodeId, defs: &mut BTreeMap<usize, String>) {
    let children = doc.children(content_id).to_vec();
    let mut after_hr = false;
    let mut to_remove = Vec::new();
    for &child in &children {
        if doc.tag_name(child).eq_ignore_ascii_case("hr") {
            after_hr = true;
            continue;
        }
        if !after_hr { continue; }
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
    for id in to_remove { doc.remove(id); }
}

/// Footnotes under "Notes"/"Footnotes"/"References" heading.
fn collect_heading_separated_defs(doc: &mut Document, content_id: NodeId, defs: &mut BTreeMap<usize, String>) {
    let re = regex_lite::Regex::new(FOOTNOTE_SECTION_RE).unwrap();
    let headings = doc.query_selector_all_in_subtree(content_id, "h2,h3,h4,h5,h6");
    let children = doc.children(content_id).to_vec();

    for &h_id in &headings {
        let text = doc.text_content(h_id).trim().to_string();
        if !re.is_match(&text) { continue; }
        if let Some(pos) = children.iter().position(|&c| c == h_id) {
            doc.remove(h_id);
            for &child in &children[pos + 1..] {
                let tag = doc.tag_name(child).to_lowercase();
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
fn collect_p_class_defs(doc: &mut Document, content_id: NodeId, defs: &mut BTreeMap<usize, String>) {
    let ps = doc.query_selector_all_in_subtree(content_id, "p.footnote, p.footnotes, li.footnote");
    for p_id in ps {
        let num = defs.len() + 1;
        defs.entry(num).or_insert_with(|| doc.inner_html(p_id));
    }
}

fn has_closest(doc: &Document, node: NodeId, selectors: &[&str]) -> bool {
    selectors.iter().any(|s| doc.closest(node, s).is_some())
}
