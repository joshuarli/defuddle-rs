use crate::dom::{Document, NodeId};
use crate::extraction_selectors::ALLOWED_ATTRIBUTES;
use rustc_hash::FxHashSet;

mod callouts;
mod elements;
mod headings;
mod images;
mod tables;
mod urls;
mod whitespace;

// Re-exports for external callers
pub(crate) use callouts::convert_alert_elements;
pub(crate) use images::get_best_src_url;
pub(crate) use images::remove_small_images;
pub(crate) use urls::resolve_urls;

use elements::{
    convert_block_spans, convert_data_as_spans, convert_embedded_elements,
    flatten_definition_lists, flatten_wrapper_elements, remove_empty_elements, remove_empty_lines,
    remove_obsolete_elements, remove_orphaned_dividers, remove_permalink_anchors,
    replace_custom_elements, resolve_svg_colors, standardize_drop_caps, standardize_element_roles,
    standardize_preformatted_elements, strip_extra_br_elements, unwrap_figure_content_wrappers,
};
use headings::{remove_trailing_headings, simplify_heading_children, standardize_headings};
use images::{
    dedup_images, normalize_srcset_attributes, recover_lazy_images, recover_noscript_images,
    remove_base64_placeholder_images, remove_empty_video_elements,
};
use tables::{normalize_data_tables, unwrap_layout_tables};
use whitespace::{move_inline_whitespace, remove_html_comments, standardize_spaces};

pub fn standardize_content(
    doc: &mut Document,
    content_id: NodeId,
    metadata_title: &str,
    overrides: Option<&crate::overrides::PageOverrides>,
) {
    let se = overrides.map(|o| &o.standardize_exceptions);

    if let Some(se) = se
        && let Some(pre) = se.pre
    {
        pre(doc, content_id);
    }

    // Step order matches TypeScript standardizeContent()
    if !se.is_some_and(|s| s.skip_drop_caps) {
        standardize_drop_caps(doc, content_id);
    }
    standardize_spaces(doc, content_id);
    move_inline_whitespace(doc, content_id);
    standardize_preformatted_elements(doc, content_id);
    remove_html_comments(doc, content_id);
    standardize_headings(doc, content_id, metadata_title);
    unwrap_layout_tables(doc, content_id);
    normalize_data_tables(doc, content_id);
    standardize_element_roles(doc, content_id);
    simplify_heading_children(doc, content_id);
    resolve_svg_colors(doc, content_id);
    replace_custom_elements(doc, content_id);
    convert_data_as_spans(doc, content_id);
    convert_block_spans(doc, content_id);
    flatten_definition_lists(doc, content_id);
    flatten_wrapper_elements(doc, content_id);
    remove_permalink_anchors(doc, content_id);
    crate::footnotes::standardize_footnotes(doc, content_id);
    recover_noscript_images(doc, content_id);
    recover_lazy_images(doc, content_id);
    strip_unwanted_attributes(doc, content_id);
    unwrap_bare_spans(doc, content_id);
    unwrap_special_links(doc, content_id);
    for figcaption in doc.query_selector_all_in_subtree(content_id, "figcaption") {
        for anchor in doc.query_selector_all_in_subtree(figcaption, "a") {
            doc.unwrap(anchor);
        }
    }
    unwrap_figure_content_wrappers(doc, content_id);
    normalize_srcset_attributes(doc, content_id);
    remove_base64_placeholder_images(doc, content_id);
    remove_empty_video_elements(doc, content_id);
    if !se.is_some_and(|s| s.skip_dedup_images) {
        dedup_images(doc, content_id);
    }
    remove_obsolete_elements(doc, content_id);
    remove_empty_elements(doc, content_id);
    remove_trailing_headings(doc, content_id);
    remove_orphaned_dividers(doc, content_id);
    flatten_wrapper_elements(doc, content_id);
    remove_orphaned_dividers(doc, content_id);
    convert_embedded_elements(doc, content_id);
    strip_extra_br_elements(doc, content_id);
    remove_empty_lines(doc, content_id);

    if let Some(se) = se
        && let Some(post) = se.post
    {
        post(doc, content_id);
    }
}

fn strip_unwanted_attributes(doc: &mut Document, content_id: NodeId) {
    let allowed: FxHashSet<&str> = ALLOWED_ATTRIBUTES.iter().copied().collect();
    let all_els = doc.query_selector_all_in_subtree(content_id, "*");
    for id in all_els {
        let names = doc.attr_names(id);
        for name in names {
            let lower = name.to_lowercase();
            if !allowed.contains(lower.as_str()) && lower != "class" && lower != "id" {
                doc.remove_attr(id, &name);
            }
        }
    }
}

fn unwrap_bare_spans(doc: &mut Document, content_id: NodeId) {
    let spans = doc.query_selector_all_in_subtree(content_id, "span");
    for id in spans {
        if doc.attr_names(id).is_empty() {
            doc.unwrap(id);
        }
    }
}

fn unwrap_special_links(doc: &mut Document, content_id: NodeId) {
    let code_links = doc.query_selector_all_in_subtree(content_id, "code a");
    for &id in &code_links {
        doc.unwrap(id);
    }
    // Use plain "a" selector since [href] attribute presence matching is buggy
    let all_links = doc.query_selector_all_in_subtree(content_id, "a");
    for &id in &all_links {
        if let Some(href) = doc.get_attr(id, "href")
            && href.trim().to_lowercase().starts_with("javascript:")
        {
            doc.unwrap(id);
        }
    }
}
