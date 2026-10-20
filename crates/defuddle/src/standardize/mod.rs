use crate::dom::{Document, NodeId};
use crate::extraction_selectors::ALLOWED_ATTRIBUTES;

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

/// Apply the attribute and spacer cleanup that is safe for HTML already built
/// by a site extractor. Extractors may preserve selected classes for consumers,
/// but page-authored styling and identity attributes have no meaning after the
/// source page's CSS is gone.
pub(crate) fn standardize_extractor_output(doc: &mut Document, content_id: NodeId, debug: bool) {
    strip_extractor_attributes(doc, content_id, debug);
    if !debug {
        remove_empty_elements(doc, content_id);
    }
    strip_extra_br_elements(doc, content_id);
}

fn strip_unwanted_attributes(doc: &mut Document, content_id: NodeId) {
    let all_els = doc.query_selector_all_in_subtree(content_id, "*");
    for id in all_els {
        doc.retain_attributes(id, |name| {
            name.eq_ignore_ascii_case("class")
                || name.eq_ignore_ascii_case("id")
                || ALLOWED_ATTRIBUTES
                    .iter()
                    .any(|attribute| name.eq_ignore_ascii_case(attribute))
        });
    }
}

fn strip_extractor_attributes(doc: &mut Document, content_id: NodeId, debug: bool) {
    let mut elements = doc.query_selector_all_in_subtree(content_id, "*");
    if doc.is_element(content_id) {
        elements.insert(0, content_id);
    }

    for id in elements {
        for name in doc.attr_names(id) {
            let value = doc.get_attr(id, &name).unwrap_or_default();

            if name.eq_ignore_ascii_case("id")
                && (value.starts_with("fnref:") || value.starts_with("fn:") || value == "footnotes")
            {
                continue;
            }
            if name.eq_ignore_ascii_case("data-defuddle") {
                continue;
            }
            if name.eq_ignore_ascii_case("class") {
                if debug {
                    continue;
                }
                let kept = value
                    .split_whitespace()
                    .filter(|token| {
                        crate::extractors::comments::is_extractor_class(token)
                            || (doc.tag_name_is(id, "code") && token.starts_with("language-"))
                            || *token == "footnote-backref"
                            || token.starts_with("callout")
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if kept.is_empty() {
                    doc.remove_attr(id, &name);
                } else if kept != value {
                    doc.set_attr(id, &name, &kept);
                }
                continue;
            }
            if debug
                && (name
                    .get(..5)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data-"))
                    || name.eq_ignore_ascii_case("class")
                    || name.eq_ignore_ascii_case("id"))
            {
                continue;
            }
            let allowed = ALLOWED_ATTRIBUTES
                .iter()
                .any(|attribute| name.eq_ignore_ascii_case(attribute));
            if !allowed {
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
            && href
                .trim()
                .get(..11)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("javascript:"))
        {
            doc.unwrap(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::standardize_extractor_output;
    use crate::dom::Document;

    #[test]
    fn extractor_output_keeps_generated_markup_and_allowed_attributes() {
        let mut doc = Document::parse(
            r#"<article data-defuddle class="reddit post page-layout" id="source">
                <div class="comment page-comment" style="color:red" data-page-state="open">
                    <a href="/thread" class="page-link" id="source-link">Comment</a>
                </div>
                <div class="empty" data-page-state="empty"></div>
            </article>"#,
        );
        let root = doc
            .query_selector_all("article")
            .first()
            .copied()
            .expect("article root");

        standardize_extractor_output(&mut doc, root, false);

        let html = doc.outer_html(root);
        assert!(html.contains("data-defuddle"));
        assert!(html.contains("class=\"reddit post\""));
        assert!(html.contains("class=\"comment\""));
        assert!(html.contains("href=\"/thread\""));
        assert!(!html.contains("page-layout"));
        assert!(!html.contains("page-comment"));
        assert!(!html.contains("style="));
        assert!(!html.contains("data-page-state"));
        assert!(!html.contains("id=\"source"));
        assert!(!html.contains("empty"));
    }
}
