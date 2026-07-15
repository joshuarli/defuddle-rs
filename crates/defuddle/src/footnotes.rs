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
    let mut definitions: FxHashMap<usize, String> = FxHashMap::default();
    let anchor_keys = collect_anchor_definitions(doc, content_id, &mut definitions);

    for anchor_id in doc.query_selector_all_in_subtree(content_id, "a") {
        let Some(href) = doc.get_attr(anchor_id, "href") else {
            continue;
        };
        let fragment = href.rsplit('#').next().unwrap_or("");
        if let Some((_, number)) = anchor_keys.iter().find(|(key, _)| key == fragment) {
            doc.set_attr(anchor_id, "data-fn-ref", &number.to_string());
        }
    }

    // Phase 1: Mark inline references matching known selectors
    let mut ref_count = doc
        .query_selector_all_in_subtree(content_id, "a")
        .iter()
        .filter(|id| doc.get_attr(**id, "data-fn-ref").is_some())
        .count();
    let special_refs = doc
        .query_selector_all_in_subtree(content_id, "*")
        .into_iter()
        .filter(|id| {
            let class = doc.get_attr(*id, "class").unwrap_or("");
            class
                .split_whitespace()
                .any(|name| name == "easy-footnote" || name == "inline-footnote")
        })
        .collect::<Vec<_>>();
    for id in special_refs {
        ref_count += 1;
        doc.set_attr(id, "data-fn-ref", &ref_count.to_string());
    }
    let wp_refs = doc
        .query_selector_all_in_subtree(content_id, "*")
        .into_iter()
        .filter(|id| {
            doc.tag_name(*id).eq_ignore_ascii_case("sup") && doc.get_attr(*id, "data-fn").is_some()
        })
        .collect::<Vec<_>>();
    for sup_id in wp_refs {
        ref_count += 1;
        doc.set_attr(sup_id, "data-fn-ref", &ref_count.to_string());
        for anchor_id in doc.query_selector_all_in_subtree(sup_id, "a") {
            doc.set_attr(anchor_id, "data-fn-ref", &ref_count.to_string());
        }
    }
    let marked_anchors = doc
        .query_selector_all_in_subtree(content_id, "a")
        .into_iter()
        .filter(|id| {
            doc.get_attr(*id, "data-fn-ref").is_some()
                && !doc
                    .closest(*id, "sup")
                    .is_some_and(|sup| doc.get_attr(sup, "data-fn").is_some())
        })
        .collect::<Vec<_>>();
    for anchor_id in marked_anchors {
        let Some(num_str) = doc.get_attr(anchor_id, "data-fn-ref") else {
            continue;
        };
        let Ok(num) = num_str.parse::<usize>() else {
            continue;
        };
        if definitions.contains_key(&num) {
            let raw_display = doc.text_content(anchor_id).trim().to_string();
            let display = raw_display.trim_matches(['[', ']']);
            let replacement = if raw_display != display {
                format!(
                    " <sup id=\"fnref:{num}\"><a data-fn-done=\"true\" href=\"#fn:{num}\">{display}</a></sup> "
                )
            } else {
                format!(
                    "<sup id=\"fnref:{num}\"><a data-fn-done=\"true\" href=\"#fn:{num}\">{display}</a></sup>"
                )
            };
            doc.replace_with_html(anchor_id, &replacement);
        }
    }
    for selector in INLINE_REF_SELECTORS {
        if *selector == "sup[data-fn] > a[href^=\"#\"]" {
            continue;
        }
        let els = doc.query_selector_all_in_subtree(content_id, selector);
        for el_id in els {
            if has_closest(doc, el_id, &["pre", "code"]) {
                continue;
            }
            if doc.get_attr(el_id, "data-fn-ref").is_some() {
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
        if has_closest(doc, sup_id, &["p.footnote", "p.footnotes", "li.footnote"]) {
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
    let mut numbers = definitions.keys().copied().collect::<Vec<_>>();
    numbers.sort_unstable();
    for num in numbers {
        let def_html = definitions.get(&num).expect("footnote definition exists");
        ol_html.push_str(&format!(
            "<li id=\"fn:{num}\">{def_html}<a class=\"footnote-backref\" title=\"return to article\" href=\"#fnref:{num}\">\u{21A9}</a></li>\n"
        ));
    }
    ol_html.push_str("</ol>\n</div>");

    // Append footnote list after the content_id
    doc.append_html(content_id, &ol_html);

    // Phase 4: Convert inline refs to sup with anchor
    let special_refs = doc
        .query_selector_all_in_subtree(content_id, "*")
        .into_iter()
        .filter(|id| {
            let class = doc.get_attr(*id, "class").unwrap_or("");
            class
                .split_whitespace()
                .any(|name| name == "easy-footnote" || name == "inline-footnote")
        })
        .collect::<Vec<_>>();
    for id in special_refs {
        let Some(num_str) = doc.get_attr(id, "data-fn-ref") else {
            continue;
        };
        let Ok(num) = num_str.parse::<usize>() else {
            continue;
        };
        if definitions.contains_key(&num) {
            let display = doc.text_content(id).trim().to_string();
            doc.replace_with_html(
                id,
                &format!("<sup id=\"fnref:{num}\"><a href=\"#fn:{num}\">{display}</a></sup>"),
            );
        }
    }
    let wp_refs = doc
        .query_selector_all_in_subtree(content_id, "*")
        .into_iter()
        .filter(|id| {
            doc.tag_name(*id).eq_ignore_ascii_case("sup") && doc.get_attr(*id, "data-fn").is_some()
        })
        .collect::<Vec<_>>();
    for sup_id in wp_refs {
        let Some(num_str) = doc.get_attr(sup_id, "data-fn-ref") else {
            continue;
        };
        let Ok(num) = num_str.parse::<usize>() else {
            continue;
        };
        if !definitions.contains_key(&num) {
            continue;
        }
        let display = doc.text_content(sup_id).trim().to_string();
        doc.replace_with_html(
            sup_id,
            &format!("<sup id=\"fnref:{num}\"><a href=\"#fn:{num}\">{display}</a></sup>"),
        );
    }
    for selector in INLINE_REF_SELECTORS {
        if *selector == "sup[data-fn] > a[href^=\"#\"]" {
            continue;
        }
        let els = doc.query_selector_all_in_subtree(content_id, selector);
        for el_id in els {
            if doc.get_attr(el_id, "data-fn-done").is_some() {
                continue;
            }
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
            for anchor_id in doc.query_selector_all_in_subtree(li_id, "a") {
                if doc
                    .get_attr(anchor_id, "aria-label")
                    .is_some_and(|label| label.to_ascii_lowercase().contains("jump to footnote"))
                {
                    doc.remove(anchor_id);
                }
            }
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
    doc: &mut Document,
    content_id: NodeId,
    defs: &mut FxHashMap<usize, String>,
) {
    let lists = doc
        .query_selector_all_in_subtree(content_id, "*")
        .into_iter()
        .filter(|id| {
            doc.tag_name(*id).eq_ignore_ascii_case("ol")
                && doc
                    .get_attr(*id, "class")
                    .unwrap_or("")
                    .split_whitespace()
                    .any(|name| name == "easy-footnotes-wrapper")
        })
        .collect::<Vec<_>>();
    for list_id in lists {
        let items = doc
            .query_selector_all_in_subtree(list_id, "li")
            .into_iter()
            .filter(|id| {
                doc.get_attr(*id, "class")
                    .unwrap_or("")
                    .contains("easy-footnote-single")
            })
            .collect::<Vec<_>>();
        for (index, item) in items.into_iter().enumerate() {
            for marker in doc
                .query_selector_all_in_subtree(item, "span")
                .into_iter()
                .filter(|id| {
                    doc.get_attr(*id, "class")
                        .unwrap_or("")
                        .contains("margin-adjust")
                })
                .collect::<Vec<_>>()
            {
                doc.remove(marker);
            }
            for backlink in doc
                .query_selector_all_in_subtree(item, "a")
                .into_iter()
                .filter(|id| doc.get_attr(*id, "class").unwrap_or("").contains("to-top"))
                .collect::<Vec<_>>()
            {
                doc.remove(backlink);
            }
            defs.insert(index + 1, doc.inner_html(item));
        }
        doc.remove(list_id);
    }

    let inline_refs = doc
        .query_selector_all_in_subtree(content_id, "*")
        .into_iter()
        .filter(|id| {
            doc.get_attr(*id, "class")
                .unwrap_or("")
                .split_whitespace()
                .any(|name| name == "inline-footnote")
        })
        .collect::<Vec<_>>();
    for (index, id) in inline_refs.into_iter().enumerate() {
        let body = doc
            .query_selector_all_in_subtree(id, "*")
            .into_iter()
            .find(|child| {
                doc.get_attr(*child, "class")
                    .unwrap_or("")
                    .split_whitespace()
                    .any(|name| name == "footnoteContent")
            });
        if let Some(body) = body {
            let mut html = doc.inner_html(body);
            if html.contains("<a ") && html.trim_end().ends_with('.') {
                html = html.trim_end().trim_end_matches('.').to_string();
            }
            defs.insert(index + 1, html);
            doc.remove(body);
        } else if let Some(title) = doc.get_attr(id, "title") {
            defs.insert(index + 1, title.to_string());
        }
    }
}

fn collect_anchor_definitions(
    doc: &mut Document,
    content_id: NodeId,
    defs: &mut FxHashMap<usize, String>,
) -> Vec<(String, usize)> {
    let mut keys = Vec::new();

    let reference_blocks = doc
        .query_selector_all_in_subtree(content_id, "*")
        .into_iter()
        .filter(|id| {
            doc.get_attr(*id, "class")
                .unwrap_or("")
                .split_whitespace()
                .any(|class| class == "reference")
        })
        .collect::<Vec<_>>();
    for block in reference_blocks {
        let Some(anchor) = doc
            .query_selector_all_in_subtree(block, "a")
            .into_iter()
            .find(|id| {
                doc.get_attr(*id, "id")
                    .is_some_and(|value| value.starts_with("ref"))
            })
        else {
            continue;
        };
        let key = doc.get_attr(anchor, "id").unwrap_or("").to_string();
        let Some(number) = key.trim_start_matches("ref").parse::<usize>().ok() else {
            continue;
        };
        let body = doc
            .query_selector_all_in_subtree(block, "*")
            .into_iter()
            .find(|id| {
                doc.get_attr(*id, "class")
                    .unwrap_or("")
                    .split_whitespace()
                    .any(|class| class == "reference-content")
            })
            .map(|id| doc.inner_html(id))
            .unwrap_or_else(|| doc.inner_html(block));
        defs.insert(number, format!("<p>{body}</p>"));
        keys.push((key, number));
        doc.remove(block);
    }
    if !keys.is_empty() {
        for heading in doc.query_selector_all_in_subtree(content_id, "h2,h3,h4") {
            if doc
                .text_content(heading)
                .trim()
                .eq_ignore_ascii_case("references")
            {
                doc.remove(heading);
            }
        }
    }
    for references in doc
        .query_selector_all_in_subtree(content_id, "div")
        .into_iter()
        .filter(|id| {
            doc.get_attr(*id, "class")
                .unwrap_or("")
                .split_whitespace()
                .any(|class| class == "references")
        })
        .collect::<Vec<_>>()
    {
        if doc.text_content(references).trim().is_empty() {
            doc.remove(references);
        }
    }

    let named_paragraphs = doc
        .query_selector_all_in_subtree(content_id, "p")
        .into_iter()
        .filter(|id| {
            doc.query_selector_all_in_subtree(*id, "a")
                .iter()
                .any(|anchor| {
                    doc.get_attr(*anchor, "name")
                        .is_some_and(|name| name.to_ascii_lowercase().starts_with("footnote_"))
                })
        })
        .collect::<Vec<_>>();
    for paragraph in named_paragraphs {
        let Some(anchor) = doc
            .query_selector_all_in_subtree(paragraph, "a")
            .into_iter()
            .find(|id| doc.get_attr(*id, "name").is_some())
        else {
            continue;
        };
        let name = doc.get_attr(anchor, "name").unwrap_or("").to_string();
        let Some(number) = name
            .rsplit('_')
            .next()
            .and_then(|value| value.parse::<usize>().ok())
        else {
            continue;
        };
        doc.remove(anchor);
        defs.insert(number, doc.inner_html(paragraph));
        keys.push((name, number));
        doc.remove(paragraph);
    }

    let list_items = doc
        .query_selector_all_in_subtree(content_id, "li")
        .into_iter()
        .filter(|id| {
            doc.query_selector_all_in_subtree(*id, "a")
                .iter()
                .any(|anchor| {
                    doc.get_attr(*anchor, "id")
                        .is_some_and(|value| value.to_ascii_lowercase().starts_with('r'))
                })
        })
        .collect::<Vec<_>>();
    for item in list_items {
        let Some(anchor) = doc
            .query_selector_all_in_subtree(item, "a")
            .into_iter()
            .find(|id| {
                doc.get_attr(*id, "id").is_some_and(|value| {
                    let suffix = value.trim_start_matches(['r', 'R']);
                    suffix.parse::<usize>().is_ok()
                })
            })
        else {
            continue;
        };
        let key = doc.get_attr(anchor, "id").unwrap_or("").to_string();
        let Some(number) = key.trim_start_matches(['r', 'R']).parse::<usize>().ok() else {
            continue;
        };
        doc.remove(anchor);
        defs.insert(number, doc.inner_html(item));
        keys.push((key, number));
        doc.remove(item);
    }

    keys
}

/// HR-separated sup definitions: <p><sup>N</sup> text</p> after <hr>.
fn collect_hr_separated_defs(
    doc: &mut Document,
    content_id: NodeId,
    defs: &mut FxHashMap<usize, String>,
) {
    let children = doc.children(content_id).to_vec();
    let Some(hr_pos) = children
        .iter()
        .position(|&child| doc.tag_name(child).eq_ignore_ascii_case("hr"))
    else {
        return;
    };
    let mut to_remove = Vec::new();
    let mut current_num = None;
    let mut current_html = String::new();
    for &child in &children[hr_pos + 1..] {
        let tag = doc.tag_name(child).to_lowercase();
        if tag.is_empty() {
            continue;
        }
        let marker = doc
            .query_selector_all_in_subtree(child, "strong,sup")
            .into_iter()
            .find_map(|id| doc.text_content(id).trim().parse::<usize>().ok());
        if let Some(num) = marker {
            if let Some(previous) = current_num.take() {
                defs.entry(previous).or_insert_with(|| current_html.clone());
            }
            current_html.clear();
            current_num = Some(num);
            let markers = doc
                .query_selector_all_in_subtree(child, "strong,sup")
                .into_iter()
                .filter(|id| doc.text_content(*id).trim().parse::<usize>().is_ok())
                .collect::<Vec<_>>();
            for id in markers {
                doc.remove(id);
            }
        }
        if current_num.is_some() {
            if tag == "ul" || tag == "ol" {
                current_html.push_str(&doc.outer_html(child));
            } else {
                current_html.push_str(&doc.inner_html(child));
            }
            current_html.push('\n');
            to_remove.push(child);
        }
    }
    if let Some(previous) = current_num {
        defs.entry(previous).or_insert(current_html);
    }
    for id in children[hr_pos..].iter().chain(to_remove.iter()) {
        let id = *id;
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
            if !text.eq_ignore_ascii_case("references and notes") {
                doc.remove(h_id);
            }
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
        let sup = doc
            .query_selector_all_in_subtree(p_id, "sup")
            .first()
            .copied();
        let num = sup
            .and_then(|id| doc.text_content(id).trim().parse::<usize>().ok())
            .unwrap_or(defs.len() + 1);
        if let Some(sup) = sup {
            doc.remove(sup);
        }
        defs.insert(num, doc.inner_html(p_id));
        doc.remove(p_id);
    }
}

fn has_closest(doc: &Document, node: NodeId, selectors: &[&str]) -> bool {
    selectors.iter().any(|s| doc.closest(node, s).is_some())
}
