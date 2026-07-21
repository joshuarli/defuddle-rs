use crate::dom::{Document, NodeId};
use rustc_hash::FxHashSet;
use std::sync::OnceLock;

static CODE_MIRROR_FONT_MEDIUM_LANG_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static CODE_MIRROR_HEADER_LANG_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static CODE_MIRROR_SPAN_LANG_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static CODE_GAP_COLLAPSE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static STRIP_HTML_TAGS_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static LINE_NUMBER_SPAN_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static USER_SELECT_SPAN_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static EMPTY_SPAN_RE: OnceLock<regex_lite::Regex> = OnceLock::new();

pub(crate) fn wrap_preformatted_code(doc: &mut Document, content_id: NodeId) {
    let codes = doc.query_selector_all_in_subtree(content_id, "code");
    for code_id in codes {
        // Skip if already inside a <pre>
        if doc.closest(code_id, "pre").is_some() {
            continue;
        }
        // Check for white-space: pre or white-space:pre style
        let style = doc.get_attr(code_id, "style").unwrap_or("").to_lowercase();
        if !style.contains("white-space") {
            continue;
        }
        // Match "white-space:pre" or "white-space: pre" followed by not "pre-" variants
        let is_pre = {
            let rest = style.split("white-space").nth(1).unwrap_or("");
            let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ':');
            rest.starts_with("pre") && !rest.starts_with("pre-")
        };
        if !is_pre {
            continue;
        }
        // Wrap in a <pre> element
        let attrs: Vec<(String, String)> = doc
            .attr_names(code_id)
            .iter()
            .filter_map(|name| {
                doc.get_attr(code_id, name)
                    .map(|v| (name.clone(), v.to_string()))
            })
            .collect();
        let inner = doc.inner_html(code_id);
        let trimmed = inner.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Clear code element
        for child in doc.children(code_id).to_vec() {
            doc.remove(child);
        }

        let mut pre_html = String::from("<pre><code");
        for (name, val) in &attrs {
            if name != "style" {
                pre_html.push_str(&format!(r#" {}="{}""#, name, val));
            }
        }
        pre_html.push('>');
        pre_html.push_str(trimmed);
        pre_html.push_str("</code></pre>");

        doc.replace_with_html(code_id, &pre_html);
    }
}

/// Convert CodeMirror editor blocks (used by ChatGPT, Claude, etc.) to standard <pre><code>.
pub(crate) fn normalize_codemirror_blocks(doc: &mut Document, content_id: NodeId) {
    // Find <pre> elements containing CodeMirror content
    let pres: Vec<NodeId> = doc
        .query_selector_all_in_subtree(content_id, "pre")
        .into_iter()
        .filter(|&id| {
            let html = doc.inner_html(id);
            html.contains("cm-editor") || html.contains("cm-content") || html.contains("CodeMirror")
        })
        .collect();

    for pre_id in pres {
        // Look for the language label in the CodeMirror toolbar before inspecting code tokens.
        let mut language = String::new();
        let html = doc.inner_html(pre_id);
        for id in doc.query_selector_all_in_subtree(pre_id, "div") {
            let is_language_label = doc
                .get_attr(id, "class")
                .is_some_and(|class| class.split_whitespace().any(|part| part == "font-medium"));
            if is_language_label {
                let candidate = doc.text_content(id).trim().to_lowercase();
                if !candidate.is_empty()
                    && candidate
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '#' || c == '-')
                {
                    language = candidate;
                    break;
                }
            }
        }
        // Try to find language from text nodes near the top (e.g., "Python" in a div header)
        if language.is_empty() {
            let re = CODE_MIRROR_FONT_MEDIUM_LANG_RE.get_or_init(|| {
                regex_lite::Regex::new(
                    r#"<div[^>]*class="[^"]*font-medium[^"]*"[^>]*>(?:<[^>]+>)*([A-Za-z][A-Za-z0-9+#.-]*)"#,
                )
                .unwrap()
            });
            if let Some(caps) = re.captures(&html) {
                language = caps
                    .get(1)
                    .map(|m| m.as_str().to_lowercase())
                    .unwrap_or_default();
            }
        }
        if language.is_empty() {
            let re = CODE_MIRROR_HEADER_LANG_RE.get_or_init(|| {
                regex_lite::Regex::new(
                    r#"<div[^>]*class="[^"]*header[^"]*"[^>]*>\s*(\w+)\s*</div>"#,
                )
                .unwrap()
            });
            if let Some(caps) = re.captures(&html) {
                language = caps
                    .get(1)
                    .map(|m| m.as_str().to_lowercase())
                    .unwrap_or_default();
            }
        }
        // Also try to find language from a dedicated language-label element
        if language.is_empty() {
            let re2 = CODE_MIRROR_SPAN_LANG_RE
                .get_or_init(|| regex_lite::Regex::new(r#"<span[^>]*>\s*(\w+)\s*</span>"#).unwrap());
            if let Some(caps) = re2.captures(&html) {
                let candidate = caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                // Filter out common non-language words
                let candidate_lower = candidate.to_lowercase();
                if !matches!(candidate_lower.as_str(), "run" | "copy" | "") {
                    language = candidate_lower;
                }
            }
        }

        // Extract only the editor content. CodeMirror uses <br> for line breaks and spans for tokens.
        let content_id = doc
            .query_selector_all_in_subtree(pre_id, ".cm-content")
            .first()
            .copied();
        let mut text = if let Some(content_id) = content_id {
            doc.text_content_with_br(content_id)
        } else {
            doc.text_content(pre_id)
        };
        text = text.trim().to_string();
        let collapse_code_gaps = CODE_GAP_COLLAPSE_RE
            .get_or_init(|| regex_lite::Regex::new(r"\n{3,}").unwrap());
        text = collapse_code_gaps.replace_all(&text, "\n\n").to_string();

        if language == "def"
            && text
                .lines()
                .any(|line| line.trim_start().starts_with("def "))
        {
            language = "python".to_string();
        }

        if text.is_empty() {
            continue;
        }

        // Build <pre><code> replacement
        let escaped_text = crate::utils::escape_html(&text);
        let code_html = if !language.is_empty() {
            format!("<pre><code class=\"language-{language}\">{escaped_text}</code></pre>")
        } else {
            format!("<pre><code>{escaped_text}</code></pre>")
        };

        doc.replace_with_html(pre_id, &code_html);
    }
}

/// Standardize code blocks: convert syntax highlighter containers
/// (.highlight, .syntaxhighlighter, etc.) to clean <pre><code> elements.
pub(crate) fn standardize_code_blocks(doc: &mut Document, content_id: NodeId) {
    normalize_rouge_tables(doc, content_id);
    normalize_code_pre_nesting(doc, content_id);
    normalize_block_code_elements(doc, content_id);
    normalize_codemirror_blocks(doc, content_id);

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
            if doc.tag_name_is_any(id, &["pre", "code"]) {
                return false;
            }
            doc.get_attr(id, "class")
                .is_some_and(|c| c.to_lowercase().contains("language-"))
        })
        .collect();
    let mut seen: FxHashSet<NodeId> = FxHashSet::default();
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
                doc.get_attr(*cid, "class").is_some_and(|c| {
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
            if let Some(parent) = doc.parent(*pre_id)
                && doc.is_element(parent)
            {
                let parent_class = doc.get_attr(parent, "class").unwrap_or("").to_lowercase();
                // For Chroma: both columns have lntd, so check if this pre is the FIRST td child
                if parent_class.contains("lntd")
                    && let Some(grandparent) = doc.parent(parent)
                {
                    let siblings: Vec<NodeId> = doc.children(grandparent).to_vec();
                    // If this is NOT the last td, it's likely the gutter
                    if siblings.last() != Some(&parent) {
                        continue;
                    }
                }
            }
            let inner = clean_code_html_for_text(&doc.inner_html(*pre_id));
            let text = inner
                .replace("<br>", "\n")
                .replace("<br/>", "\n")
                .replace("<br />", "\n");
            let re = STRIP_HTML_TAGS_RE
                .get_or_init(|| regex_lite::Regex::new(r"<[^>]*>").unwrap());
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
        let escaped = crate::utils::escape_html(&code_content);
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
        if !doc.tag_name_is_any(id, &["code", "pre"]) {
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
            if !dl.is_empty()
                && dl.len() < 20
                && dl != "plaintext"
                && dl != "plain"
                && dl != "text"
                && dl != "none"
            {
                // Set class for htmd compatibility
                let existing = doc.get_attr(id, "class").unwrap_or("");
                let new_class = format!("{} language-{}", existing, dl);
                doc.set_attr(id, "class", &new_class);
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

pub(crate) fn normalize_code_pre_nesting(doc: &mut Document, content_id: NodeId) {
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

pub(crate) fn normalize_bare_pre_code_blocks(doc: &mut Document, content_id: NodeId) {
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
        let escaped = crate::utils::escape_html(&code_content);
        let new_html = format!("<pre><code{}>{}</code></pre>", lang_attr, escaped);
        doc.replace_with_html(pre_id, &new_html);
    }
}

pub(crate) fn normalize_block_code_elements(doc: &mut Document, content_id: NodeId) {
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
            crate::utils::escape_html(text.trim())
        );
        doc.replace_with_html(code_id, &new_html);
    }
}

pub(crate) fn normalize_rouge_tables(doc: &mut Document, content_id: NodeId) {
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
            crate::utils::escape_html(text.trim_end())
        );
        doc.replace_with_html(table_id, &new_html);
    }
}

pub(crate) fn copy_button_code(doc: &Document, root: NodeId) -> Option<String> {
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

pub(crate) fn remove_code_controls(doc: &mut Document, root: NodeId) {
    for selector in ["button", "style"] {
        for id in doc.query_selector_all_in_subtree(root, selector) {
            doc.remove(id);
        }
    }
}

pub(crate) fn data_language(doc: &Document, id: NodeId) -> Option<String> {
    let value = doc.get_attr(id, "data-language")?.trim();
    if value.is_empty() || matches!(value, "plaintext" | "plain" | "text" | "none") {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn ancestor_language(doc: &Document, id: NodeId) -> String {
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

pub(crate) fn extract_code_lang(class: &str) -> String {
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
    if class.starts_with("highlight ")
        && let Some(lang) = class.split_whitespace().nth(1)
    {
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
    String::new()
}

pub(crate) fn clean_code_html_for_text(html: &str) -> String {
    let line_number_re = LINE_NUMBER_SPAN_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r#"<span[^>]*class="[^"]*(?:lineno|line-number|line-number-[^"]*)[^"]*"[^>]*>.*?</span>"#,
        )
        .unwrap()
    });
    let html = line_number_re.replace_all(html, "").to_string();
    let user_select_re = USER_SELECT_SPAN_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r#"<span[^>]*style="[^"]*user-select\s*:\s*none[^"]*"[^>]*>.*?</span>"#,
        )
        .unwrap()
    });
    let html = user_select_re.replace_all(&html, "").to_string();
    let empty_span_re = EMPTY_SPAN_RE
        .get_or_init(|| regex_lite::Regex::new(r#"<span[^>]*></span>"#).unwrap());
    empty_span_re.replace_all(&html, "").to_string()
}

pub(crate) fn code_text_from_html(html: &str) -> String {
    let html = clean_code_html_for_text(html);
    let text = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    let re = STRIP_HTML_TAGS_RE
        .get_or_init(|| regex_lite::Regex::new(r"<[^>]*>").unwrap());
    re.replace_all(&text, "")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
}

pub(crate) fn dedent_code(code: &str) -> String {
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

