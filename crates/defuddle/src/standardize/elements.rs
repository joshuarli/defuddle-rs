use crate::dom::{Document, NodeId};
use crate::extraction_selectors::{
    ALLOWED_EMPTY_ELEMENTS, BLOCK_ELEMENTS, BLOCK_LEVEL_ELEMENTS, INLINE_ELEMENTS,
    PRESERVE_ELEMENTS,
};
use rustc_hash::FxHashMap;

fn tag_in(tag: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| tag.eq_ignore_ascii_case(candidate))
}

/// Preserve author-declared preformatted code when it is represented as a
/// standalone `<code style="white-space: pre">` element.
pub(crate) fn standardize_preformatted_elements(doc: &mut Document, content_id: NodeId) {
    let codes = doc.query_selector_all_in_subtree(content_id, "code");
    for code_id in codes {
        if doc.closest(code_id, "pre").is_some() {
            continue;
        }
        let style = doc.get_attr(code_id, "style").unwrap_or("").to_lowercase();
        let Some(value) = style.split("white-space").nth(1) else {
            continue;
        };
        let value = value.trim_start_matches(|c: char| c.is_whitespace() || c == ':');
        if !value.starts_with("pre") || value.starts_with("pre-") {
            continue;
        }

        let inner = doc.inner_html(code_id);
        let inner = inner.trim();
        if inner.is_empty() {
            continue;
        }
        let attrs: Vec<(String, String)> = doc
            .attr_names(code_id)
            .into_iter()
            .filter(|name| name != "style")
            .filter_map(|name| {
                doc.get_attr(code_id, &name)
                    .map(|value| (name, value.to_string()))
            })
            .collect();

        let mut replacement = String::from("<pre><code");
        for (name, value) in attrs {
            replacement.push_str(&format!(r#" {}="{}""#, name, value));
        }
        replacement.push('>');
        replacement.push_str(inner);
        replacement.push_str("</code></pre>");
        doc.replace_with_html(code_id, &replacement);
    }
}

/// Unwrap <figure> elements that contain block-level content (paragraphs, headings),

pub(crate) fn unwrap_figure_content_wrappers(doc: &mut Document, content_id: NodeId) {
    let figures = doc.query_selector_all_in_subtree(content_id, "figure");
    for id in figures {
        let children = doc.children(id).to_vec();
        let has_block = children.iter().any(|&c| {
            if !doc.is_element(c) {
                return false;
            }
            doc.tag_name_is_any(
                c,
                &[
                    "p",
                    "h1",
                    "h2",
                    "h3",
                    "h4",
                    "h5",
                    "h6",
                    "ul",
                    "ol",
                    "blockquote",
                    "pre",
                ],
            )
        });
        if has_block {
            doc.unwrap(id);
        }
    }
}

/// When an <img> has a base64/SVG placeholder src and a sibling <noscript> contains

pub(crate) fn remove_obsolete_elements(doc: &mut Document, content_id: NodeId) {
    for tag in &["object", "embed", "applet"] {
        for id in doc.query_selector_all_in_subtree(content_id, tag) {
            doc.remove(id);
        }
    }
}

/// Remove empty elements (no text, no children, not in ALLOWED_EMPTY_ELEMENTS)

pub(crate) fn remove_empty_elements(doc: &mut Document, content_id: NodeId) {
    loop {
        let mut removed = false;
        let all = doc.query_selector_all_in_subtree(content_id, "*");
        for id in all {
            if id == content_id {
                continue;
            }
            let tag = doc.tag_name(id);
            let is_table = tag.eq_ignore_ascii_case("table");
            if tag_in(tag, ALLOWED_EMPTY_ELEMENTS) {
                continue;
            }
            if doc.has_selector_in_subtree(id, "h1,h2,h3,h4,h5,h6") {
                continue;
            }
            if doc.tag_name_is_any(id, &["pre", "code"]) {
                continue;
            }
            if doc.children(id).is_empty() && doc.text_content(id).trim().is_empty() {
                doc.remove(id);
                removed = true;
            }
            // Remove empty tables (no visible text content in any cell)
            if is_table {
                let text = doc.text_content(id).trim().to_string();
                if text.is_empty() {
                    doc.remove(id);
                    removed = true;
                }
            }
        }
        if !removed {
            break;
        }
    }
}

pub(crate) fn remove_orphaned_dividers(doc: &mut Document, content_id: NodeId) {
    let direct = doc.children(content_id).to_vec();
    let leading_hrs: Vec<NodeId> = direct
        .iter()
        .take_while(|&&id| {
            doc.is_text(id) && doc.text_content(id).trim().is_empty()
                || doc.is_element(id) && doc.tag_name(id).eq_ignore_ascii_case("hr")
        })
        .copied()
        .filter(|&id| doc.is_element(id) && doc.tag_name(id).eq_ignore_ascii_case("hr"))
        .collect();
    for id in leading_hrs {
        if doc.is_element(id) && doc.tag_name(id).eq_ignore_ascii_case("hr") {
            doc.remove(id);
        }
    }
    let direct = doc.children(content_id).to_vec();
    let trailing_hrs: Vec<NodeId> = direct
        .iter()
        .rev()
        .take_while(|&&id| {
            doc.is_text(id) && doc.text_content(id).trim().is_empty()
                || doc.is_element(id) && doc.tag_name(id).eq_ignore_ascii_case("hr")
        })
        .copied()
        .filter(|&id| doc.is_element(id) && doc.tag_name(id).eq_ignore_ascii_case("hr"))
        .collect();
    for id in trailing_hrs {
        if doc.is_element(id) && doc.tag_name(id).eq_ignore_ascii_case("hr") {
            doc.remove(id);
        }
    }
    for id in doc.query_selector_all_in_subtree(content_id, "hr") {
        let Some(parent) = doc.parent(id) else {
            continue;
        };
        let siblings = doc.children(parent).to_vec();
        let Some(pos) = siblings.iter().position(|&s| s == id) else {
            continue;
        };
        let next = siblings[pos + 1..].iter().find(|&&s| {
            doc.is_element(s) || doc.is_text(s) && !doc.text_content(s).trim().is_empty()
        });
        if let Some(&next) = next
            && doc.is_element(next)
            && doc.tag_name(next).eq_ignore_ascii_case("hr")
        {
            doc.remove(next);
        }
    }
}

pub(crate) fn flatten_wrapper_elements(doc: &mut Document, content_id: NodeId) {
    let has_inline_content = |doc: &Document, id: NodeId| -> bool {
        for &child in doc.children(id) {
            if doc.is_text(child) && !doc.text_content(child).trim().is_empty() {
                return true;
            }
            if doc.is_element(child) && tag_in(doc.tag_name(child), INLINE_ELEMENTS) {
                return true;
            }
        }
        false
    };

    let should_keep = |doc: &Document, id: NodeId| -> bool {
        let tag = doc.tag_name(id);
        if doc.is_svg_element(id) || tag_in(tag, PRESERVE_ELEMENTS) {
            return true;
        }
        if doc.get_attr(id, "id") == Some("footnotes") {
            return true;
        }
        if doc.get_attr(id, "data-callout").is_some() {
            return true;
        }
        if let Some(role) = doc.get_attr(id, "role")
            && matches!(
                role,
                "article" | "main" | "navigation" | "banner" | "contentinfo"
            )
        {
            return true;
        }
        if let Some(class) = doc.get_attr(id, "class") {
            if crate::content_utils::contains_ascii_case_insensitive(class, "article")
                || crate::content_utils::contains_ascii_case_insensitive(class, "main")
                || crate::content_utils::contains_ascii_case_insensitive(class, "content")
                || crate::content_utils::contains_ascii_case_insensitive(class, "footnote")
                || crate::content_utils::contains_ascii_case_insensitive(class, "reference")
                || crate::content_utils::contains_ascii_case_insensitive(class, "bibliography")
                || crate::content_utils::contains_ascii_case_insensitive(class, "callout-")
            {
                return true;
            }
        }
        false
    };

    loop {
        let mut changed = false;
        // Build candidates by querying each block element tag individually
        let mut candidates: Vec<NodeId> = Vec::new();
        for tag in BLOCK_ELEMENTS {
            for id in doc.query_selector_all_in_subtree(content_id, tag) {
                if id != content_id
                    && tag_in(doc.tag_name(id), BLOCK_ELEMENTS)
                    && !should_keep(doc, id)
                {
                    candidates.push(id);
                }
            }
        }
        for id in candidates {
            if has_inline_content(doc, id) {
                continue;
            }
            let el_children: Vec<NodeId> = doc
                .children(id)
                .to_vec()
                .into_iter()
                .filter(|&c| doc.is_element(c))
                .collect();
            if el_children.is_empty() {
                continue;
            }
            if el_children.len() == 1 {
                if tag_in(doc.tag_name(el_children[0]), BLOCK_LEVEL_ELEMENTS)
                    && !should_keep(doc, el_children[0])
                {
                    doc.unwrap(id);
                    changed = true;
                    continue;
                }
            }
            if el_children
                .iter()
                .all(|&c| tag_in(doc.tag_name(c), BLOCK_LEVEL_ELEMENTS))
            {
                doc.unwrap(id);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Normalize <br> tags: remove standalone brs between blocks, collapse consecutive brs,

pub(crate) fn strip_extra_br_elements(doc: &mut Document, content_id: NodeId) {
    let brs = doc.query_selector_all_in_subtree(content_id, "br");
    let mut groups: FxHashMap<NodeId, Vec<NodeId>> = FxHashMap::default();
    for &id in &brs {
        if let Some(pid) = doc.parent(id) {
            groups.entry(pid).or_default().push(id);
        }
    }
    for (pid, mut group) in groups {
        let siblings = doc.children(pid).to_vec();
        group.sort_by_key(|&id| siblings.iter().position(|&s| s == id));
        // Remove leading brs if no preceding content
        if let Some(&first) = group.first()
            && let Some(p) = siblings.iter().position(|&s| s == first)
        {
            let has_before = siblings[..p].iter().any(|&s| {
                (doc.is_text(s) && !doc.text_content(s).trim().is_empty())
                    || (doc.is_element(s) && !doc.tag_name(s).eq_ignore_ascii_case("br"))
            });
            if !has_before {
                for &b in &group {
                    doc.remove(b);
                }
                continue;
            }
        }

        // Remove standalone <br>s between block-level siblings
        // Only strip brs between blocks when parent is a block container (div, article, section, etc.)
        // or the content root
        if pid == content_id || tag_in(doc.tag_name(pid), BLOCK_ELEMENTS) {
            for &br_id in &group {
                if let Some(pos) = siblings.iter().position(|&s| s == br_id) {
                    let before_is_block = pos > 0
                        && siblings[..pos].iter().any(|&s| {
                            doc.is_element(s)
                                && tag_in(doc.tag_name(s), BLOCK_ELEMENTS)
                                && !doc.text_content(s).trim().is_empty()
                        });
                    let after_is_block = siblings[pos + 1..].iter().any(|&s| {
                        doc.is_element(s)
                            && tag_in(doc.tag_name(s), BLOCK_ELEMENTS)
                            && !doc.text_content(s).trim().is_empty()
                    });
                    if before_is_block && after_is_block {
                        doc.remove(br_id);
                    }
                }
            }
        }

        // Collapse 3+ consecutive to 2
        let mut consecutive: Vec<NodeId> = Vec::new();
        for &child in &siblings {
            if doc.is_element(child) && doc.tag_name(child).eq_ignore_ascii_case("br") {
                consecutive.push(child);
            } else {
                if consecutive.len() > 2 {
                    for &b in &consecutive[2..] {
                        doc.remove(b);
                    }
                }
                consecutive.clear();
            }
        }
        if consecutive.len() > 2 {
            for &b in &consecutive[2..] {
                doc.remove(b);
            }
        }
    }

    // Remove <p> elements that contain only <br> (empty visual spacers)
    remove_empty_br_paragraphs(doc, content_id);
}

pub(crate) fn remove_empty_br_paragraphs(doc: &mut Document, content_id: NodeId) {
    loop {
        let mut removed = false;
        let ps = doc.query_selector_all_in_subtree(content_id, "p");
        for id in ps {
            if id == content_id {
                continue;
            }
            let children = doc.children(id).to_vec();
            let all_br = !children.is_empty()
                && children.iter().all(|&c| {
                    (doc.is_element(c) && doc.tag_name(c).eq_ignore_ascii_case("br"))
                        || (doc.is_text(c) && doc.text_content(c).trim().is_empty())
                });
            if all_br {
                doc.remove(id);
                removed = true;
            }
        }
        if !removed {
            break;
        }
    }
}

pub(crate) fn remove_empty_lines(doc: &mut Document, content_id: NodeId) {
    let children = doc.children(content_id).to_vec();
    for child in children {
        remove_empty_text_nodes(doc, child);
    }
}

pub(crate) fn remove_empty_text_nodes(doc: &mut Document, node: NodeId) {
    if doc.is_element(node) {
        if doc.tag_name_is_any(node, &["pre", "code"]) {
            return;
        }
        let children = doc.children(node).to_vec();
        for child in children {
            remove_empty_text_nodes(doc, child);
        }
    }
    let text_children: Vec<NodeId> = doc
        .children(node)
        .to_vec()
        .into_iter()
        .filter(|&c| doc.is_text(c))
        .collect();
    let mut normalized = String::new();
    for tc in text_children {
        let text = doc.text_content(tc);
        let is_invis = text.trim().is_empty()
            || text.chars().all(|c| {
                matches!(
                    c,
                    '\u{200C}' | '\u{200B}' | '\u{200D}' | '\u{200E}' | '\u{200F}' | '\u{FEFF}'
                )
            });
        if is_invis {
            doc.remove(tc);
        } else {
            crate::content_utils::normalize_whitespace_into(&text, &mut normalized);
            if normalized != text {
                doc.set_text(tc, &normalized);
            }
        }
    }
}

pub(crate) fn replace_custom_elements(doc: &mut Document, content_id: NodeId) {
    let custom: Vec<NodeId> = doc
        .query_selector_all_in_subtree(content_id, "*")
        .into_iter()
        .filter(|&id| {
            let name = doc.tag_name(id);
            name.contains('-') && !tag_in(name, INLINE_ELEMENTS) && !doc.is_svg_element(id)
        })
        .collect();
    for id in custom.iter().rev() {
        doc.set_tag_name(*id, "div");
    }
}

pub(crate) fn convert_data_as_spans(doc: &mut Document, content_id: NodeId) {
    let allowed = ["p", "h1", "h2", "h3", "h4", "h5", "h6", "li", "blockquote"];
    let spans = doc
        .query_selector_all_in_subtree(content_id, "*")
        .into_iter()
        .filter(|id| {
            doc.tag_name(*id).eq_ignore_ascii_case("span") && doc.get_attr(*id, "data-as").is_some()
        })
        .collect::<Vec<_>>();
    for id in spans {
        let target = doc.get_attr(id, "data-as").unwrap_or("").to_lowercase();
        if !target.is_empty() && tag_in(&target, &allowed) {
            doc.set_tag_name(id, &target);
        }
    }
}

pub(crate) fn convert_block_spans(doc: &mut Document, content_id: NodeId) {
    let spans = doc.query_selector_all_in_subtree(content_id, "span");
    for id in spans {
        let class = doc.get_attr(id, "class").unwrap_or("");
        let style = doc.get_attr(id, "style").unwrap_or("");
        let is_block = crate::content_utils::contains_ascii_case_insensitive(class, "block")
            || crate::content_utils::contains_ascii_case_insensitive(style, "display:block")
            || crate::content_utils::contains_ascii_case_insensitive(style, "display: block");
        if is_block && !doc.text_content(id).trim().is_empty() {
            doc.set_tag_name(id, "p");
        }
    }
}

pub(crate) fn standardize_drop_caps(doc: &mut Document, content_id: NodeId) {
    let caps = doc.query_selector_all_in_subtree(content_id, r#"span[data-caps="initial"]"#);
    for id in caps {
        // Since we can't easily insert text nodes between siblings in the current DOM,
        // we just unwrap the span and remove the following <small>
        // Simplified: just unwrap the span
        doc.unwrap(id);
    }
}

pub(crate) fn remove_permalink_anchors(doc: &mut Document, content_id: NodeId) {
    for level in 1..=6 {
        let headings = doc.query_selector_all_in_subtree(content_id, &format!("h{level}"));
        for h_id in headings {
            let anchors: Vec<NodeId> = doc
                .children(h_id)
                .to_vec()
                .into_iter()
                .filter(|&c| doc.is_element(c) && doc.tag_name(c).eq_ignore_ascii_case("a"))
                .collect();
            for a_id in anchors {
                let text = doc.text_content(a_id).trim().to_string();
                if text == "#" || text == "¶" || text == "§" || text.is_empty() {
                    doc.remove(a_id);
                } else if !doc
                    .get_attr(a_id, "class")
                    .unwrap_or("")
                    .split_whitespace()
                    .any(|class| matches!(class, "permalink" | "headerlink" | "anchor-link"))
                {
                    doc.unwrap(a_id);
                }
            }
        }
    }
}

pub(crate) fn convert_embedded_elements(doc: &mut Document, content_id: NodeId) {
    crate::embeds::youtube::standardize_lite_youtube(doc, content_id);
    // Add controls attribute to videos without them
    let videos = doc.query_selector_all_in_subtree(content_id, "video");
    for id in videos {
        if doc.get_attr(id, "controls").is_none() {
            doc.set_attr(id, "controls", "");
        }
    }
}

pub(crate) fn standardize_element_roles(doc: &mut Document, content_id: NodeId) {
    // div[role="paragraph"] → p
    let paras = doc.query_selector_all_in_subtree(content_id, r#"div[role="paragraph"]"#);
    for id in paras {
        doc.set_tag_name(id, "p");
    }

    // div[role="list"] → ul/ol
    let lists = doc.query_selector_all_in_subtree(content_id, r#"div[role="list"]"#);
    for id in lists {
        // Check first listitem label for ordered list
        let listitems = doc.query_selector_all_in_subtree(id, r#"div[role="listitem"]"#);
        let is_ordered = listitems.first().is_some_and(|&li_id| {
            let label = doc.query_selector_all_in_subtree(li_id, ".label");
            label.first().is_some_and(|&lbl_id| {
                let binding = doc.text_content(lbl_id);
                let text = binding.trim();
                text.starts_with(|c: char| c.is_ascii_digit()) && text.contains(')')
            })
        });
        doc.set_tag_name(id, if is_ordered { "ol" } else { "ul" });
    }

    // div[role="listitem"] → li
    let items = doc.query_selector_all_in_subtree(content_id, r#"div[role="listitem"]"#);
    for id in items {
        doc.set_tag_name(id, "li");
    }
}

/// Flatten definition lists (<dl>) by converting <dt>/<dd> pairs into

pub(crate) fn flatten_definition_lists(doc: &mut Document, content_id: NodeId) {
    let dls = doc.query_selector_all_in_subtree(content_id, "dl");
    for dl_id in dls {
        if doc.is_removed(dl_id) {
            continue;
        }
        // Collect dt/dd pairs
        let children: Vec<NodeId> = doc.children(dl_id).to_vec();
        // Build replacement HTML
        let mut replacement = String::new();
        for child_id in children {
            if doc.tag_name_is(child_id, "dt") {
                let html = doc.inner_html(child_id);
                // Wrap dt content on its own line (will become inline code in markdown)
                replacement.push_str(&html);
                replacement.push('\n');
            } else if doc.tag_name_is(child_id, "dd") {
                let html = doc.inner_html(child_id);
                replacement.push_str(&format!("<p>{html}</p>\n"));
            } else {
                let outer = doc.outer_html(child_id);
                replacement.push_str(&outer);
            }
        }
        doc.replace_with_html(dl_id, &replacement);
    }
}

pub(crate) fn resolve_svg_colors(doc: &mut Document, content_id: NodeId) {
    let tailwind_colors: &[(&str, &str)] = &[
        ("slate", "#64748b"),
        ("gray", "#6b7280"),
        ("zinc", "#71717a"),
        ("neutral", "#737373"),
        ("stone", "#78716c"),
        ("red", "#ef4444"),
        ("orange", "#f97316"),
        ("amber", "#f59e0b"),
        ("yellow", "#eab308"),
        ("lime", "#84cc16"),
        ("green", "#22c55e"),
        ("emerald", "#10b981"),
        ("teal", "#14b8a6"),
        ("cyan", "#06b6d4"),
        ("sky", "#0ea5e9"),
        ("blue", "#3b82f6"),
        ("indigo", "#6366f1"),
        ("violet", "#8b5cf6"),
        ("purple", "#a855f7"),
        ("fuchsia", "#d946ef"),
        ("pink", "#ec4899"),
        ("rose", "#f43f5e"),
    ];

    let svg_els = doc.query_selector_all_in_subtree(content_id, "svg");
    for svg_id in svg_els {
        let fills = doc.query_selector_all_in_subtree(svg_id, "[class]");
        for id in fills {
            let class = doc.get_attr(id, "class").unwrap_or("").to_string();
            for (color_name, hex) in tailwind_colors {
                let prefix = format!("fill-{color_name}-");
                if class.contains(&prefix) {
                    doc.set_attr(id, "fill", hex);
                }
                let stroke_prefix = format!("stroke-{color_name}-");
                if class.contains(&stroke_prefix) {
                    doc.set_attr(id, "stroke", hex);
                }
                // Handle text-* → fill for SVG
                let text_prefix = format!("text-{color_name}-");
                if class.contains(&text_prefix) {
                    doc.set_attr(id, "fill", hex);
                }
            }
        }
    }
}
