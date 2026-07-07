use crate::constants::{
    ALLOWED_ATTRIBUTES, ALLOWED_EMPTY_ELEMENTS, BLOCK_ELEMENTS, BLOCK_LEVEL_ELEMENTS,
    INLINE_ELEMENTS, PRESERVE_ELEMENTS,
};
use crate::dom::{Document, NodeId};
use std::collections::HashSet;

// --- Public entry point ---

pub fn standardize_content(doc: &mut Document, content_id: NodeId, metadata_title: &str) {
    // Step order matches TypeScript standardizeContent()
    standardize_drop_caps(doc, content_id);
    standardize_spaces(doc, content_id);
    remove_html_comments(doc, content_id);
    standardize_headings(doc, content_id, metadata_title);
    standardize_code_blocks(doc, content_id);
    standardize_element_roles(doc, content_id);
    simplify_heading_children(doc, content_id);
    resolve_svg_colors(doc, content_id);
    replace_custom_elements(doc, content_id);
    convert_data_as_spans(doc, content_id);
    convert_block_spans(doc, content_id);
    unwrap_layout_tables(doc, content_id);
    flatten_wrapper_elements(doc, content_id);
    remove_permalink_anchors(doc, content_id);
    strip_unwanted_attributes(doc, content_id);
    unwrap_bare_spans(doc, content_id);
    unwrap_special_links(doc, content_id);
    remove_obsolete_elements(doc, content_id);
    remove_empty_elements(doc, content_id);
    remove_trailing_headings(doc, content_id);
    remove_orphaned_dividers(doc, content_id);
    flatten_wrapper_elements(doc, content_id);
    remove_orphaned_dividers(doc, content_id);
    convert_embedded_elements(doc, content_id);
    strip_extra_br_elements(doc, content_id);
    remove_empty_lines(doc, content_id);
}

/// Resolve relative URLs in element attributes to absolute URLs using the page URL.
pub fn resolve_urls(doc: &mut Document, content_id: NodeId, page_url: &str) {
    if page_url.is_empty() {
        return;
    }

    let base_url = page_url.to_string();
    if !base_url.contains("://") {
        return;
    }

    let url_attrs = ["src", "href", "srcset", "poster", "data-src", "data-srcset"];
    let all_els = doc.query_selector_all_in_subtree(content_id, "*");

    for id in all_els {
        for attr_name in &url_attrs {
            if let Some(value) = doc.get_attr(id, attr_name) {
                let trimmed = value.trim();
                if trimmed.is_empty()
                    || trimmed.starts_with("data:")
                    || trimmed.starts_with("javascript:")
                {
                    continue;
                }
                if let Some(resolved) = resolve_url(trimmed, &base_url) {
                    doc.set_attr(id, attr_name, &resolved);
                }
            }
        }
    }
}

// --- Individual standardization steps ---

/// Replace &nbsp; with regular spaces in text nodes.
fn standardize_spaces(doc: &mut Document, node: NodeId) {
    let children = doc.children(node).to_vec();
    for child in children {
        if doc.is_text(child) {
            if let Some(text) = doc.text_content_mut(child) {
                *text = text.replace('\u{00A0}', " ");
            }
        } else if doc.is_element(child) {
            standardize_spaces(doc, child);
        }
    }
}

/// Normalize headings: h1 → h2, remove duplicate title heading, remove trailing headings.
fn standardize_headings(doc: &mut Document, content_id: NodeId, metadata_title: &str) {
    // Promote h1 → h2 (same as upstream)
    for level in 1..=1 {
        for id in doc.query_selector_all_in_subtree(content_id, &format!("h{level}")) {
            doc.set_tag_name(id, "h2");
        }
    }

    // Remove first h2 if its text matches the metadata title (upstream behavior)
    if !metadata_title.is_empty() {
        let h2s = doc.query_selector_all_in_subtree(content_id, "h2");
        if let Some(&first_h2) = h2s.first() {
            let heading_text = crate::utils::normalize_text(&doc.text_content(first_h2));
            let normalized_title = crate::utils::normalize_text(metadata_title);
            if !normalized_title.is_empty() && heading_text == normalized_title {
                doc.remove(first_h2);
            }
        }
    }
}

/// Strip attributes not in the allowed set.
fn strip_unwanted_attributes(doc: &mut Document, content_id: NodeId) {
    let allowed: HashSet<&str> = ALLOWED_ATTRIBUTES.iter().copied().collect();
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

/// Unwrap spans that have no attributes.
fn unwrap_bare_spans(doc: &mut Document, content_id: NodeId) {
    let spans = doc.query_selector_all_in_subtree(content_id, "span");
    for id in spans {
        if doc.attr_names(id).is_empty() {
            doc.unwrap(id);
        }
    }
}

/// Handle special link unwrapping: code links, javascript: links.
fn unwrap_special_links(doc: &mut Document, content_id: NodeId) {
    let code_links = doc.query_selector_all_in_subtree(content_id, "code a");
    for id in code_links {
        doc.unwrap(id);
    }
    let all_links = doc.query_selector_all_in_subtree(content_id, "a[href]");
    for id in all_links {
        if let Some(href) = doc.get_attr(id, "href") {
            if href.trim().to_lowercase().starts_with("javascript:") {
                doc.unwrap(id);
            }
        }
    }
}

/// Remove obsolete elements in content context.
fn remove_obsolete_elements(doc: &mut Document, content_id: NodeId) {
    for tag in &["object", "embed", "applet"] {
        for id in doc.query_selector_all_in_subtree(content_id, tag) {
            doc.remove(id);
        }
    }
}

/// Remove empty elements (no text, no children, not in ALLOWED_EMPTY_ELEMENTS).
fn remove_empty_elements(doc: &mut Document, content_id: NodeId) {
    let allowed_empty: HashSet<&str> = ALLOWED_EMPTY_ELEMENTS.iter().copied().collect();
    loop {
        let mut removed = false;
        let all = doc.query_selector_all_in_subtree(content_id, "*");
        for id in all {
            if id == content_id {
                continue;
            }
            let tag = doc.tag_name(id).to_lowercase();
            if allowed_empty.contains(tag.as_str()) {
                continue;
            }
            if tag == "pre" || tag == "code" {
                continue;
            }
            if doc.children(id).is_empty() && doc.text_content(id).trim().is_empty() {
                doc.remove(id);
                removed = true;
            }
        }
        if !removed {
            break;
        }
    }
}

/// Remove trailing headings at end of content.
fn remove_trailing_headings(doc: &mut Document, content_id: NodeId) {
    let children = doc.children(content_id).to_vec();
    let mut last_idx: Option<usize> = None;
    for (i, &child) in children.iter().enumerate() {
        if doc.is_element(child) {
            let tag = doc.tag_name(child).to_lowercase();
            if !matches!(tag.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "hr") {
                if !doc.text_content(child).trim().is_empty() {
                    last_idx = Some(i);
                }
            }
        } else if doc.is_text(child) && !doc.text_content(child).trim().is_empty() {
            last_idx = Some(i);
        }
    }
    if let Some(idx) = last_idx {
        for &c in &children[idx + 1..] {
            if doc.is_element(c)
                && matches!(
                    doc.tag_name(c).to_lowercase().as_str(),
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                )
            {
                doc.remove(c);
            }
        }
    }
}

/// Remove orphaned <hr> dividers.
fn remove_orphaned_dividers(doc: &mut Document, content_id: NodeId) {
    let hrs = doc.query_selector_all_in_subtree(content_id, "hr");
    for id in hrs {
        let Some(pid) = doc.parent(id) else { continue };
        let siblings = doc.children(pid).to_vec();
        let Some(pos) = siblings.iter().position(|&s| s == id) else {
            continue;
        };
        let has_before = siblings[..pos]
            .iter()
            .any(|&s| doc.is_element(s) && !doc.text_content(s).trim().is_empty());
        let has_after = siblings[pos + 1..]
            .iter()
            .any(|&s| doc.is_element(s) && !doc.text_content(s).trim().is_empty());
        if !has_before && !has_after {
            doc.remove(id);
        }
    }
}

/// Flatten unnecessary wrapper divs/sections.
fn flatten_wrapper_elements(doc: &mut Document, content_id: NodeId) {
    let block_set: HashSet<&str> = BLOCK_ELEMENTS.iter().copied().collect();
    let block_level: HashSet<&str> = BLOCK_LEVEL_ELEMENTS.iter().copied().collect();
    let preserve: HashSet<&str> = PRESERVE_ELEMENTS.iter().copied().collect();
    let inline: HashSet<&str> = INLINE_ELEMENTS.iter().copied().collect();

    let has_inline_content = |doc: &Document, id: NodeId| -> bool {
        for &child in doc.children(id) {
            if doc.is_text(child) && !doc.text_content(child).trim().is_empty() {
                return true;
            }
            if doc.is_element(child) && inline.contains(doc.tag_name(child).to_lowercase().as_str())
            {
                return true;
            }
        }
        false
    };

    let should_keep = |doc: &Document, id: NodeId| -> bool {
        let tag = doc.tag_name(id).to_lowercase();
        if doc.is_svg_element(id) || preserve.contains(tag.as_str()) {
            return true;
        }
        if doc.get_attr(id, "data-callout").is_some() {
            return true;
        }
        if let Some(role) = doc.get_attr(id, "role") {
            if matches!(
                role,
                "article" | "main" | "navigation" | "banner" | "contentinfo"
            ) {
                return true;
            }
        }
        if let Some(class) = doc.get_attr(id, "class") {
            let l = class.to_lowercase();
            if l.contains("article")
                || l.contains("main")
                || l.contains("content")
                || l.contains("footnote")
                || l.contains("reference")
                || l.contains("bibliography")
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
                    && block_set.contains(doc.tag_name(id).to_lowercase().as_str())
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
                let ct = doc.tag_name(el_children[0]).to_lowercase();
                if block_level.contains(ct.as_str()) && !should_keep(doc, el_children[0]) {
                    doc.unwrap(id);
                    changed = true;
                    continue;
                }
            }
            if el_children
                .iter()
                .all(|&c| block_level.contains(doc.tag_name(c).to_lowercase().as_str()))
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

/// Normalize consecutive <br> tags.
fn strip_extra_br_elements(doc: &mut Document, content_id: NodeId) {
    let brs = doc.query_selector_all_in_subtree(content_id, "br");
    let mut groups: std::collections::HashMap<NodeId, Vec<NodeId>> =
        std::collections::HashMap::new();
    for &id in &brs {
        if let Some(pid) = doc.parent(id) {
            groups.entry(pid).or_default().push(id);
        }
    }
    for (pid, mut group) in groups {
        let siblings = doc.children(pid).to_vec();
        group.sort_by_key(|&id| siblings.iter().position(|&s| s == id));
        // Remove leading brs if no preceding content
        if let Some(&first) = group.first() {
            if let Some(p) = siblings.iter().position(|&s| s == first) {
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
}

/// Remove empty text nodes and normalize whitespace.
fn remove_empty_lines(doc: &mut Document, content_id: NodeId) {
    let children = doc.children(content_id).to_vec();
    for child in children {
        remove_empty_text_nodes(doc, child);
    }
}

fn remove_empty_text_nodes(doc: &mut Document, node: NodeId) {
    if doc.is_element(node) {
        let tag = doc.tag_name(node).to_lowercase();
        if tag == "pre" || tag == "code" {
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
            let new_text = text
                .replace('\n', " ")
                .replace('\r', " ")
                .replace('\t', " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if new_text != text {
                doc.set_text(tc, &new_text);
            }
        }
    }
}

fn remove_html_comments(doc: &mut Document, node: NodeId) {
    let children = doc.children(node).to_vec();
    for child in children {
        if doc.is_comment(child) {
            doc.remove(child);
        } else if doc.is_element(child) {
            remove_html_comments(doc, child);
        }
    }
}

/// Resolve a single URL against a base URL.
fn resolve_url(url: &str, base: &str) -> Option<String> {
    if url.contains("://") {
        return Some(url.to_string());
    }
    if url.starts_with("//") {
        let proto = base.split("://").next()?;
        return Some(format!("{}:{}", proto, url));
    }
    if url.starts_with('/') {
        if let Some(end) = base.find("://") {
            let after = &base[end + 3..];
            if let Some(slash) = after.find('/') {
                return Some(format!("{}{}", &base[..end + 3 + slash], url));
            }
            return Some(format!("{}{}", &base[..end + 3], url));
        }
        return None;
    }
    if url.starts_with('#') || url.starts_with('?') || url.starts_with("mailto:") {
        return Some(url.to_string());
    }
    let last_slash = base.rfind('/');
    if let Some(pos) = last_slash {
        if pos > base.find("://").unwrap_or(0) + 2 {
            let base_dir = &base[..pos + 1];
            return Some(resolve_dot_segments(&format!("{}{}", base_dir, url)));
        }
    }
    Some(format!("{}/{}", base.trim_end_matches('/'), url))
}

fn resolve_dot_segments(path: &str) -> String {
    let mut segs: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                segs.pop();
            }
            _ => segs.push(seg),
        }
    }
    segs.join("/")
}

// --- Remaining Phase 2 functions ---

/// Replace custom elements (hyphenated tag names) with divs.
fn replace_custom_elements(doc: &mut Document, content_id: NodeId) {
    let inline_set: HashSet<&str> = INLINE_ELEMENTS.iter().copied().collect();
    let custom: Vec<NodeId> = doc
        .query_selector_all_in_subtree(content_id, "*")
        .into_iter()
        .filter(|&id| {
            let name = doc.tag_name(id);
            name.contains('-')
                && !inline_set.contains(name.to_lowercase().as_str())
                && !doc.is_svg_element(id)
        })
        .collect();
    for id in custom.iter().rev() {
        doc.set_tag_name(*id, "div");
    }
}

/// Convert <span data-as="..."> to real elements.
fn convert_data_as_spans(doc: &mut Document, content_id: NodeId) {
    let allowed: HashSet<&str> = ["p", "h1", "h2", "h3", "h4", "h5", "h6", "li", "blockquote"]
        .iter()
        .copied()
        .collect();
    let spans = doc.query_selector_all_in_subtree(content_id, "span[data-as]");
    for id in spans {
        if let Some(target) = doc.get_attr(id, "data-as") {
            let target = target.to_lowercase();
            if allowed.contains(target.as_str()) {
                doc.set_tag_name(id, &target);
            }
        }
    }
}

/// Convert display:block spans to <p> elements.
fn convert_block_spans(doc: &mut Document, content_id: NodeId) {
    let spans = doc.query_selector_all_in_subtree(content_id, "span");
    for id in spans {
        let class = doc.get_attr(id, "class").unwrap_or("").to_lowercase();
        let style = doc.get_attr(id, "style").unwrap_or("").to_lowercase();
        let is_block = class.contains("block")
            || style.contains("display:block")
            || style.contains("display: block");
        if is_block && !doc.text_content(id).trim().is_empty() {
            doc.set_tag_name(id, "p");
        }
    }
}

/// Merge drop cap elements (<span data-caps="initial">T</span><small>HE REST</small>).
fn standardize_drop_caps(doc: &mut Document, content_id: NodeId) {
    let caps = doc.query_selector_all_in_subtree(content_id, r#"span[data-caps="initial"]"#);
    for id in caps {
        let parent = doc.parent(id);
        // Since we can't easily insert text nodes between siblings in the current DOM,
        // we just unwrap the span and remove the following <small>
        let children = doc.children(content_id).to_vec(); // not ideal; skip for now
        // Simplified: just unwrap the span
        doc.unwrap(id);
    }
}

/// Remove permalink anchors from headings (links containing # ¶ § as sole text).
fn remove_permalink_anchors(doc: &mut Document, content_id: NodeId) {
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
                if text == "#" || text == "¶" || text == "§" || text == "¶" || text.is_empty() {
                    doc.remove(a_id);
                }
            }
        }
    }
}

/// Convert known embedded element placeholders to real elements.
fn convert_embedded_elements(doc: &mut Document, content_id: NodeId) {
    // Handle lite-youtube → iframe with src
    let yt = doc.query_selector_all_in_subtree(content_id, "lite-youtube");
    for id in yt {
        if let Some(videoid) = doc.get_attr(id, "videoid") {
            let src = format!("https://www.youtube.com/embed/{}", videoid);
            doc.set_tag_name(id, "iframe");
            doc.set_attr(id, "src", &src);
            doc.set_attr(id, "frameborder", "0");
            doc.set_attr(id, "allowfullscreen", "");
        }
    }
    // Add controls attribute to videos without them
    let videos = doc.query_selector_all_in_subtree(content_id, "video");
    for id in videos {
        if doc.get_attr(id, "controls").is_none() {
            doc.set_attr(id, "controls", "");
        }
    }
}

/// Convert div[role="paragraph"] to <p> and div[role="list"] to <ul>/<ol>.
fn standardize_element_roles(doc: &mut Document, content_id: NodeId) {
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
        let is_ordered = listitems.first().map_or(false, |&li_id| {
            let label = doc.query_selector_all_in_subtree(li_id, ".label");
            label.first().map_or(false, |&lbl_id| {
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

/// Unwrap single-column layout tables (tables used for positioning, not data).
fn unwrap_layout_tables(doc: &mut Document, content_id: NodeId) {
    let tables = doc.query_selector_all_in_subtree(content_id, "table");
    for table_id in tables {
        let has_structural = doc.query_selector_all_in_subtree(table_id, "thead,tfoot,th,caption");
        if !has_structural.is_empty() {
            continue;
        }
        let all_cells = doc.query_selector_all_in_subtree(table_id, "td");
        let non_empty: Vec<NodeId> = all_cells
            .into_iter()
            .filter(|&cid| !doc.text_content(cid).trim().is_empty())
            .collect();
        if non_empty.len() != 1 {
            continue;
        }
        let cell = non_empty[0];
        let cell_children: Vec<NodeId> = doc
            .children(cell)
            .to_vec()
            .into_iter()
            .filter(|&cid| doc.is_element(cid) && !doc.text_content(cid).trim().is_empty())
            .collect();
        if cell_children.len() == 1 {
            let child = cell_children[0];
            let child_tag = doc.tag_name(child).to_lowercase();
            if BLOCK_LEVEL_ELEMENTS.contains(&child_tag.as_str()) {
                doc.replace_with_html(table_id, &doc.outer_html(child));
            }
        }
    }
}

/// Standardize code blocks: convert syntax highlighter containers
/// (.highlight, .syntaxhighlighter, etc.) to clean <pre><code> elements.
fn standardize_code_blocks(doc: &mut Document, content_id: NodeId) {
    normalize_rouge_tables(doc, content_id);
    normalize_code_pre_nesting(doc, content_id);
    normalize_block_code_elements(doc, content_id);

    let highlight = doc.query_selector_all_in_subtree(content_id, ".highlight");
    let syntax = doc.query_selector_all_in_subtree(content_id, ".syntaxhighlighter");
    let wp = doc.query_selector_all_in_subtree(
        content_id,
        ".wp-block-code,.wp-block-syntaxhighlighter-code",
    );
    let lang_els: Vec<NodeId> = doc
        .query_selector_all_in_subtree(content_id, "[class]")
        .into_iter()
        .filter(|&id| {
            let tag = doc.tag_name(id).to_lowercase();
            if tag == "pre" || tag == "code" {
                return false;
            }
            doc.get_attr(id, "class")
                .map_or(false, |c| c.to_lowercase().contains("language-"))
        })
        .collect();
    let mut seen: HashSet<NodeId> = HashSet::new();
    let containers: Vec<NodeId> = highlight
        .into_iter()
        .chain(syntax)
        .chain(wp)
        .chain(lang_els)
        .filter(|id| seen.insert(*id))
        .collect();

    for container_id in containers {
        let class = doc
            .get_attr(container_id, "class")
            .unwrap_or("")
            .to_lowercase();
        let language = extract_code_lang(&class);
        let pres = doc.query_selector_all_in_subtree(container_id, "pre");
        if pres.is_empty() {
            continue;
        }
        let mut parts: Vec<String> = Vec::new();
        for pre_id in &pres {
            let pc = doc.get_attr(*pre_id, "class").unwrap_or("").to_lowercase();
            // Check if this pre is a gutter/line-number column
            if pc.contains("gutter")
                || pc.contains("line-number")
                || pc.contains("lineno")
                || pc.contains("lntable")
            {
                continue;
            }
            // Also check inner code for gutter classes (Chroma uses <code class="lnt">)
            let codes = doc.query_selector_all_in_subtree(*pre_id, "code");
            let is_gutter = codes.iter().any(|cid| {
                doc.get_attr(*cid, "class").map_or(false, |c| {
                    let c = c.to_lowercase();
                    c.contains("lnt")
                        || c.contains("ln ")
                        || c.contains("lineno")
                        || c.contains("line-number")
                })
            });
            if is_gutter {
                continue;
            }
            // Also skip if parent td has lntd class (Chroma gutter column)
            if let Some(parent) = doc.parent(*pre_id) {
                if doc.is_element(parent) {
                    let parent_class = doc.get_attr(parent, "class").unwrap_or("").to_lowercase();
                    // For Chroma: both columns have lntd, so check if this pre is the FIRST td child
                    if parent_class.contains("lntd") {
                        if let Some(grandparent) = doc.parent(parent) {
                            let siblings: Vec<NodeId> = doc.children(grandparent).to_vec();
                            // If this is NOT the last td, it's likely the gutter
                            if siblings.last() != Some(&parent) {
                                continue;
                            }
                        }
                    }
                }
            }
            let inner = clean_code_html_for_text(&doc.inner_html(*pre_id));
            let text = inner
                .replace("<br>", "\n")
                .replace("<br/>", "\n")
                .replace("<br />", "\n");
            let re = regex_lite::Regex::new(r"<[^>]*>").unwrap();
            let text = re.replace_all(&text, "").to_string();
            let text = text
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&");
            parts.push(text);
        }
        if parts.is_empty() {
            continue;
        }
        let code_content = dedent_code(&parts.join("\n"));

        // Fallback: if no language from container, check inner <pre>/<code>
        let language = if language.is_empty() {
            let mut lang = String::new();
            for pre_id in &pres {
                let pc = doc.get_attr(*pre_id, "class").unwrap_or("").to_lowercase();
                lang = extract_code_lang(&pc);
                if !lang.is_empty() {
                    break;
                }
                for cid in doc.query_selector_all_in_subtree(*pre_id, "code") {
                    let cc = doc.get_attr(cid, "class").unwrap_or("").to_lowercase();
                    lang = extract_code_lang(&cc);
                    if !lang.is_empty() {
                        break;
                    }
                }
                if !lang.is_empty() {
                    break;
                }
            }
            lang
        } else {
            language
        };
        let lang_attr = if language.is_empty() {
            String::new()
        } else {
            format!(" class=\"language-{}\"", language)
        };
        let escaped = html_escape(&code_content);
        let new_html = format!("<pre><code{}>{}</code></pre>", lang_attr, escaped);
        doc.replace_with_html(container_id, &new_html);
    }

    normalize_bare_pre_code_blocks(doc, content_id);

    // Also set data-lang on bare <pre>/<code> from class or data-language
    let all_ids: Vec<NodeId> = doc.query_selector_all_in_subtree(content_id, "*");
    for &id in &all_ids {
        if !doc.is_element(id) {
            continue;
        }
        let tag = doc.tag_name(id).to_lowercase();
        if tag != "code" && tag != "pre" {
            continue;
        }
        if doc.get_attr(id, "data-lang").is_some() {
            continue;
        }
        // Check data-language attribute (rehype-pretty-code uses this)
        let data_lang_val: Option<String> =
            doc.get_attr(id, "data-language").map(|s| s.to_string());
        if let Some(ref dl) = data_lang_val {
            let dl = dl.trim();
            if !dl.is_empty() && dl.len() < 20 {
                if dl != "plaintext" && dl != "plain" && dl != "text" && dl != "none" {
                    // Set class for htmd compatibility
                    let existing = doc.get_attr(id, "class").unwrap_or("");
                    let new_class = format!("{} language-{}", existing, dl);
                    doc.set_attr(id, "class", &new_class);
                }
            }
            continue;
        }
        if let Some(c) = doc.get_attr(id, "class") {
            let lang = extract_code_lang(&c.to_lowercase());
            if !lang.is_empty() {
                doc.set_attr(id, "data-lang", &lang);
            }
        }
    }
}

fn normalize_code_pre_nesting(doc: &mut Document, content_id: NodeId) {
    let code_nodes = doc.query_selector_all_in_subtree(content_id, "code");
    for code_id in code_nodes {
        if doc.closest(code_id, "pre").is_some() {
            continue;
        }
        let pres = doc.query_selector_all_in_subtree(code_id, "pre");
        let Some(&pre_id) = pres.first() else {
            continue;
        };
        let outer_class = doc.get_attr(code_id, "class").unwrap_or("").to_string();
        let outer_lang = extract_code_lang(&outer_class.to_lowercase());
        if !outer_lang.is_empty() {
            for inner_code in doc.query_selector_all_in_subtree(pre_id, "code") {
                if doc.get_attr(inner_code, "class").is_none() {
                    doc.set_attr(inner_code, "class", &format!("language-{outer_lang}"));
                }
            }
        }
        let html = doc.outer_html(pre_id);
        doc.replace_with_html(code_id, &html);
    }
}

fn normalize_bare_pre_code_blocks(doc: &mut Document, content_id: NodeId) {
    let pres = doc.query_selector_all_in_subtree(content_id, "pre");
    for pre_id in pres {
        if doc.is_removed(pre_id) {
            continue;
        }
        let copy_button_code = copy_button_code(doc, pre_id);
        remove_code_controls(doc, pre_id);
        let class = doc.get_attr(pre_id, "class").unwrap_or("").to_lowercase();
        let codes = doc.query_selector_all_in_subtree(pre_id, "code");
        let code_class = codes
            .first()
            .and_then(|id| doc.get_attr(*id, "class"))
            .unwrap_or("")
            .to_lowercase();
        let html = doc.inner_html(pre_id);
        let should_normalize = html.contains("line-number")
            || html.contains("linenumber")
            || html.contains("user-select")
            || html.contains("<span")
            || class.contains("chroma")
            || code_class.contains("language-");
        if !should_normalize {
            continue;
        }

        let language = data_language(doc, pre_id)
            .or_else(|| codes.first().and_then(|id| data_language(doc, *id)))
            .unwrap_or_else(|| extract_code_lang(&format!("{class} {code_class}")));
        let text = copy_button_code.unwrap_or_else(|| code_text_from_html(&html));
        let code_content = dedent_code(&text);
        let lang_attr = if language.is_empty() {
            String::new()
        } else {
            format!(" class=\"language-{}\"", language)
        };
        let escaped = html_escape(&code_content);
        let new_html = format!("<pre><code{}>{}</code></pre>", lang_attr, escaped);
        doc.replace_with_html(pre_id, &new_html);
    }
}

fn normalize_block_code_elements(doc: &mut Document, content_id: NodeId) {
    let code_nodes = doc.query_selector_all_in_subtree(content_id, "code");
    for code_id in code_nodes {
        if doc.is_removed(code_id) || doc.closest(code_id, "pre").is_some() {
            continue;
        }
        let class = doc.get_attr(code_id, "class").unwrap_or("").to_lowercase();
        let is_block = class.split_whitespace().any(|part| part == "block");
        if !is_block {
            continue;
        }
        let language = if class.split_whitespace().any(|part| part == "lean") {
            "lean".to_string()
        } else {
            extract_code_lang(&class)
        };
        let text = code_text_from_html(&doc.inner_html(code_id));
        let lang_attr = if language.is_empty() {
            String::new()
        } else {
            format!(" class=\"language-{}\"", language)
        };
        let new_html = format!(
            "<pre><code{}>{}</code></pre>",
            lang_attr,
            html_escape(text.trim())
        );
        doc.replace_with_html(code_id, &new_html);
    }
}

fn normalize_rouge_tables(doc: &mut Document, content_id: NodeId) {
    let tables = doc.query_selector_all_in_subtree(content_id, "table");
    for table_id in tables {
        if doc.is_removed(table_id) {
            continue;
        }
        let code_cells = doc.query_selector_all_in_subtree(table_id, ".rouge-code");
        let Some(&code_cell) = code_cells.first() else {
            continue;
        };
        let pres = doc.query_selector_all_in_subtree(code_cell, "pre");
        let Some(&pre_id) = pres.first() else {
            continue;
        };
        let language = ancestor_language(doc, table_id);
        let text = code_text_from_html(&doc.inner_html(pre_id));
        let lang_attr = if language.is_empty() {
            String::new()
        } else {
            format!(" class=\"language-{}\"", language)
        };
        let new_html = format!(
            "<pre><code{}>{}</code></pre>",
            lang_attr,
            html_escape(text.trim_end())
        );
        doc.replace_with_html(table_id, &new_html);
    }
}

fn copy_button_code(doc: &Document, root: NodeId) -> Option<String> {
    doc.query_selector_all_in_subtree(root, "button")
        .into_iter()
        .find_map(|id| {
            let class = doc.get_attr(id, "class").unwrap_or("");
            if class
                .split_whitespace()
                .any(|part| part == "rehype-pretty-copy")
            {
                doc.get_attr(id, "data").map(String::from)
            } else {
                None
            }
        })
}

fn remove_code_controls(doc: &mut Document, root: NodeId) {
    for selector in ["button", "style"] {
        for id in doc.query_selector_all_in_subtree(root, selector) {
            doc.remove(id);
        }
    }
}

fn data_language(doc: &Document, id: NodeId) -> Option<String> {
    let value = doc.get_attr(id, "data-language")?.trim();
    if value.is_empty() || matches!(value, "plaintext" | "plain" | "text" | "none") {
        None
    } else {
        Some(value.to_string())
    }
}

fn ancestor_language(doc: &Document, id: NodeId) -> String {
    let mut current = Some(id);
    while let Some(node) = current {
        let class = doc.get_attr(node, "class").unwrap_or("").to_lowercase();
        let lang = extract_code_lang(&class);
        if !lang.is_empty() {
            return lang;
        }
        current = doc.parent(node);
    }
    String::new()
}

fn extract_code_lang(class: &str) -> String {
    for prefix in &[
        "language-",
        "lang-",
        "brush:",
        "syntax-",
        "highlight-",
        "code-",
    ] {
        if let Some(pos) = class.find(prefix) {
            let rest = &class[pos + prefix.len()..];
            if let Some(lang) = rest.split(|c: char| c.is_whitespace()).next() {
                let lang = lang.trim_matches('-');
                if !lang.is_empty() && lang.len() < 20 {
                    // Skip non-language identifiers
                    if lang == "plaintext"
                        || lang == "plain"
                        || lang == "text"
                        || lang == "nohighlight"
                        || lang == "none"
                        || lang == "wrap"
                    {
                        return String::new();
                    }
                    return lang.to_string();
                }
            }
        }
    }
    if class.starts_with("highlight ") {
        if let Some(lang) = class.split_whitespace().nth(1) {
            let lang = lang.trim_matches('-');
            if !lang.is_empty() && lang.len() < 20 {
                if lang == "plaintext"
                    || lang == "plain"
                    || lang == "text"
                    || lang == "nohighlight"
                    || lang == "none"
                    || lang == "wrap"
                {
                    return String::new();
                }
                return lang.to_string();
            }
        }
    }
    String::new()
}

fn clean_code_html_for_text(html: &str) -> String {
    let line_number_re = regex_lite::Regex::new(
        r#"<span[^>]*class="[^"]*(?:lineno|line-number|line-number-[^"]*)[^"]*"[^>]*>.*?</span>"#,
    )
    .unwrap();
    let html = line_number_re.replace_all(html, "").to_string();
    let user_select_re = regex_lite::Regex::new(
        r#"<span[^>]*style="[^"]*user-select\s*:\s*none[^"]*"[^>]*>.*?</span>"#,
    )
    .unwrap();
    let html = user_select_re.replace_all(&html, "").to_string();
    let empty_span_re = regex_lite::Regex::new(r#"<span[^>]*></span>"#).unwrap();
    empty_span_re.replace_all(&html, "").to_string()
}

fn code_text_from_html(html: &str) -> String {
    let html = clean_code_html_for_text(html);
    let text = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    let re = regex_lite::Regex::new(r"<[^>]*>").unwrap();
    re.replace_all(&text, "")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
}

fn dedent_code(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Simplify headings by removing navigation child elements (buttons, permalink anchors).
fn simplify_heading_children(doc: &mut Document, content_id: NodeId) {
    for level in 1..=6 {
        let headings = doc.query_selector_all_in_subtree(content_id, &format!("h{level}"));
        for h_id in headings {
            let children = doc.children(h_id).to_vec();
            for child in children {
                if !doc.is_element(child) {
                    continue;
                }
                let tag = doc.tag_name(child).to_lowercase();
                let class = doc.get_attr(child, "class").unwrap_or("").to_lowercase();
                // Remove buttons, empty anchors (# links), permalink elements
                if tag == "button"
                    || (tag == "a"
                        && doc
                            .get_attr(child, "href")
                            .map_or(false, |h| h.starts_with('#')))
                    || class.contains("permalink")
                    || class.contains("anchor-link")
                    || class.contains("heading-anchor")
                {
                    doc.remove(child);
                }
            }
        }
    }
}

/// Resolve Tailwind CSS color classes in SVG elements.
fn resolve_svg_colors(doc: &mut Document, content_id: NodeId) {
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
            let mut new_class = class.clone();
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
