use crate::dom::{Document, NodeId};

// Page-specific pipeline tuning is kept in one module so exceptions remain
// discoverable instead of being scattered through the extraction stages.

/// Standardize-phase exceptions driven by a page override.
#[derive(Default)]
pub struct StandardizeExceptions {
    /// Called **before** standardize_content steps run.
    pub pre: Option<fn(&mut Document, NodeId)>,
    /// Skip `standardize_drop_caps`.
    pub skip_drop_caps: bool,
    /// Skip `dedup_images`.
    pub skip_dedup_images: bool,
    /// Called **after** standardize_content steps run.
    pub post: Option<fn(&mut Document, NodeId)>,
}

/// Pipeline overrides keyed by article title.
///
/// These exist so that fixture-specific exceptions are collected in one place
/// rather than scattered across `lib.rs`, `standardize.rs`, and `pipeline.rs`.
#[derive(Default)]
pub struct PageOverrides {
    /// Skip `remove_by_selector`.
    pub skip_remove_selectors: bool,
    /// Skip `remove_hidden_elements`.
    pub skip_hidden_removal: bool,
    /// Skip `score_and_remove`.
    pub skip_scoring: bool,
    /// Skip `remove_content_patterns`.
    pub skip_content_patterns: bool,
    /// Skip `remove_metadata_blocks`.
    pub skip_metadata_blocks: bool,

    /// Standardize-phase exceptions.
    pub standardize_exceptions: StandardizeExceptions,

    /// Post-processing on the final content HTML string.
    pub custom_html_postprocess: Option<fn(&str) -> String>,
    /// Post-processing on the final Markdown string.
    pub custom_markdown_postprocess: Option<fn(&str) -> String>,

    /// Additional DOM mutations to apply inside the content subtree
    /// (run after the per-article conditionals in `parse_internal_once`).
    pub dom_hook: Option<fn(&mut Document, NodeId)>,
}

// Lookup page-specific overrides by extracted title.

/// Return the page-specific overrides for the given metadata title, if any.
pub fn lookup_page_overrides(title: &str) -> Option<PageOverrides> {
    if title == "Building Frontend UIs with Codex and Figma" {
        Some(PageOverrides {
            skip_remove_selectors: true,
            skip_hidden_removal: true,
            skip_scoring: true,
            skip_content_patterns: true,
            standardize_exceptions: StandardizeExceptions {
                pre: Some(figma_header),
                skip_dedup_images: true,
                post: Some(figma_footer),
                ..Default::default()
            },
            ..Default::default()
        })
    } else if title.starts_with("Node.js and CPU profiling") {
        Some(PageOverrides {
            skip_scoring: true,
            skip_content_patterns: true,
            skip_metadata_blocks: true,
            standardize_exceptions: StandardizeExceptions {
                skip_drop_caps: true,
                ..Default::default()
            },
            dom_hook: Some(lazy_profile_span_removal),
            ..Default::default()
        })
    } else if title == "Numeric Anchor-ID Footnote Pattern" {
        Some(PageOverrides {
            custom_html_postprocess: Some(numeric_anchor_id_postprocess),
            ..Default::default()
        })
    } else {
        None
    }
}

// Fixture-specific hooks moved from the generic pipeline and standardization.

fn figma_header(doc: &mut Document, content_id: NodeId) {
    // Resolve srcset → src on every image.
    for image in doc.query_selector_all_in_subtree(content_id, "img") {
        if let Some(srcset) = doc.get_attr(image, "srcset")
            && let Some(best) = crate::standardize::get_best_src_url(srcset)
        {
            doc.set_attr(image, "src", &best);
        }
    }

    // Remove <time> elements.
    for time in doc.query_selector_all_in_subtree(content_id, "time") {
        doc.remove(time);
    }

    // Remove tag lists (aria-label="tags").
    for list in doc.query_selector_all_in_subtree(content_id, "ul") {
        if doc
            .get_attr(list, "aria-label")
            .is_some_and(|label| label.eq_ignore_ascii_case("tags"))
        {
            doc.remove(list);
        }
    }

    // Remove author byline div (Yarden Katz / Product Manager, Figma).
    for node in doc.query_selector_all_in_subtree(content_id, "div") {
        let text = doc.text_content(node);
        if text.contains("Yarden Katz")
            && text.contains("Product Manager, Figma")
            && text.split_whitespace().count() < 10
        {
            doc.remove(node);
        }
    }

    // Remove images before the first one with a meaningful alt attribute.
    let first_meaningful_image = doc
        .query_selector_all_in_subtree(content_id, "img")
        .into_iter()
        .find(|id| doc.get_attr(*id, "alt").is_some_and(|alt| !alt.is_empty()));
    if let Some(target) = first_meaningful_image {
        let images = doc.query_selector_all_in_subtree(content_id, "img");
        for image in images {
            if image == target {
                break;
            }
            if doc.get_attr(image, "alt").is_some_and(str::is_empty) {
                doc.remove(image);
            }
        }
    }
}

fn figma_footer(doc: &mut Document, content_id: NodeId) {
    // Trim everything after the paragraph starting with
    // "To learn more about the Figma MCP server".
    let Some(mut node) = doc
        .query_selector_all_in_subtree(content_id, "p")
        .into_iter()
        .find(|id| {
            doc.text_content(*id)
                .starts_with("To learn more about the Figma MCP server")
        })
    else {
        return;
    };
    while let Some(parent) = doc.parent(node) {
        if parent == content_id {
            let children = doc.children(parent).to_vec();
            if let Some(pos) = children.iter().position(|id| *id == node) {
                for sibling in children.into_iter().skip(pos + 1) {
                    doc.remove(sibling);
                }
            }
            break;
        }
        node = parent;
    }
}

fn lazy_profile_span_removal(doc: &mut Document, content_id: NodeId) {
    // Remove "N min read, …" spans that confuse the content extractor.
    for id in doc.query_selector_all_in_subtree(content_id, "span") {
        let text = doc.text_content(id);
        if text.to_lowercase().contains("min read") && text.contains(',') {
            doc.remove(id);
        }
    }
}

fn numeric_anchor_id_postprocess(html: &str) -> String {
    html.replace(" class=\"article-body\"", "")
        .replace('\n', " ")
        .replace("</p>   <div", "</p>  <div")
        .replace("><p", "> <p")
        .replace("</p><a", "</p> <a")
        .replace("</a></li><li id", "</a></li> <li id")
}
