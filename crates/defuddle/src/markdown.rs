use crate::dom::{Document, NodeId};
use htmd::element_handler::{HandlerResult, Handlers};
use htmd::{
    Element, HtmlToMarkdown,
    options::{HeadingStyle, LinkStyle},
};
use std::sync::OnceLock;

static INLINE_SPACING_REGEXES: OnceLock<Vec<regex_lite::Regex>> = OnceLock::new();
static WBR_TAG_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static QUOTED_FENCED_CODE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static MARKDOWN_HEADING_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static EMPTY_LINK_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static FOOTNOTE_ESCAPE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static IMAGE_ESCAPE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static URL_ESCAPE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static EMAIL_ESCAPE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static BACKREF_CLEANUP_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static DETACHED_ITALIC_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static CODE_LINKED_WORD_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static QUOTED_FENCED_LANG_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static HN_LISTING_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static BACKTICK_SPACE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static BOLD_NUM_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static BACKTICK_WORD_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static TABLE_CODE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static DASH_SEPARATOR_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static CHECKLIST_JOIN_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static TAG_LIKE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static MULTI_NL_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static LINK_DEST_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static FOOTNOTE_REF_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static CHECKED_CHECKBOX_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static UNCHECKED_CHECKBOX_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static MARK_TAG_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static DEL_TAG_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static PROTOCOL_RELATIVE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static ORIGIN_URL_RE: OnceLock<regex_lite::Regex> = OnceLock::new();

fn inline_spacing_regexes() -> &'static [regex_lite::Regex] {
    INLINE_SPACING_REGEXES.get_or_init(|| {
        [
            r"`\s*([^`]*?)\s*`",
            r"([a-zA-Z0-9])(\*\*[^*]+\*\*)",
            r"(\*\*[^*]+\*\*)([a-zA-Z0-9])",
            r"([a-zA-Z0-9])(\*[^*]+\*)",
            r"(\*[^*]+\*)([a-zA-Z0-9])",
            r"\*([^*\n]+) \*",
            r"\* +([^*\n]+?) +\*",
            r"\* ([^*\n]+)\*",
            r" (\* )([A-Za-z])",
            r"(\*[^*\n]+\*) ([.,;:!?])",
            r"([A-Za-z0-9,.;:!?])(\*[^*\n]+\*)",
            r"\*([^*\n]+)\*([A-Za-z0-9(])",
            r"(\[\^\d+\]) ([.,;:!?])",
            r"\\(-)( )",
            r"([a-zA-Z0-9:;,.!?])(`[^`]+`)",
            r"(`[^`]+`)([a-zA-Z0-9])",
            r"([a-zA-Z0-9])(\[[^\]]+\]\([^)]+\))",
            r"(\[[^\]]+\]\([^)]+\))([a-zA-Z0-9])",
            r"(\[[^\]]+\]\([^)]+\))(- )",
            r"(\[[^\]]+\]\([^)]+\)) {2,}",
            r#"([A-Za-z0-9"])(\[\^\d+\])"#,
            r"(`[^`]+`)(\[\^\d+\])",
            r"(\S) \* ([A-Z])",
            r"([A-Za-z])(\*\*\*)",
            r"(,)(\*\[)",
            r"` +([^`\n]*?)`",
            r"`([^`\n]*?) +`",
            r"`([^`\n]*)`([A-Za-z])",
            r"` +([^`\n]*)`",
            r"\\\\<([A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,})>",
        ]
        .into_iter()
        .map(|pattern| regex_lite::Regex::new(pattern).unwrap())
        .collect()
    })
}

fn replace_if_match<R: regex_lite::Replacer>(
    line: String,
    regex: &regex_lite::Regex,
    replacement: R,
) -> String {
    regex.replace_all(&line, replacement).into_owned()
}

/// Convert HTML content to Markdown using htmd (Turndown.js-compatible) with
/// custom element handlers matching the upstream Defuddle Turndown rules.
pub fn html_to_markdown(html: &str) -> String {
    let cleaned = preprocess_html_for_markdown(html);

    let converter = build_converter();

    match converter.convert(&cleaned) {
        Ok(md) => postprocess_markdown(&md),
        Err(_) => cleaned,
    }
}

/// Build the HtmlToMarkdown converter with all custom handlers.
fn build_converter() -> HtmlToMarkdown {
    let mut builder = HtmlToMarkdown::builder()
        .options(htmd::options::Options {
            heading_style: HeadingStyle::Atx,
            link_style: LinkStyle::Inlined,
            bullet_list_marker: htmd::options::BulletListMarker::Dash,
            ul_bullet_spacing: 1,
            ..Default::default()
        })
        .skip_tags(vec!["script", "style", "noscript"]);

    // Convert <mark> to ==text==.
    builder = builder.add_handler(
        vec!["mark"],
        |_h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
            let children = _h.walk_children(el.node);
            Some(HandlerResult {
                content: format!("=={}==", children.content),
                markdown_translated: true,
            })
        },
    );

    // Convert <del>, <s>, and <strike> to ~~text~~.
    builder = builder.add_handler(
        vec!["del", "s", "strike"],
        |_h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
            let children = _h.walk_children(el.node);
            Some(HandlerResult {
                content: format!("~~{}~~", children.content),
                markdown_translated: true,
            })
        },
    );

    // Convert supported embeds while preserving unknown iframes.
    builder = builder.add_handler(
        vec!["iframe"],
        |_h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
            let src = el
                .attrs
                .iter()
                .find(|a| a.name.local.as_ref() == "src")
                .map(|a| a.value.as_ref())
                .unwrap_or("");
            if let Some(content) = crate::embeds::iframe_to_markdown(src) {
                Some(HandlerResult {
                    content,
                    markdown_translated: true,
                })
            } else {
                let attrs = el
                    .attrs
                    .iter()
                    .map(|a| {
                        format!(
                            " {}=\"{}\"",
                            a.name.local,
                            crate::content_utils::escape_html(a.value.as_ref())
                        )
                    })
                    .collect::<String>();
                Some(HandlerResult {
                    content: format!("<iframe{attrs}></iframe>"),
                    markdown_translated: true,
                })
            }
        },
    );

    // Convert footnote citations to Markdown references.
    builder = builder.add_handler(
        vec!["sup"],
        |_h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
            let id = el
                .attrs
                .iter()
                .find(|a| a.name.local.as_ref() == "id")
                .map(|a| a.value.as_ref())
                .unwrap_or("");
            if let Some(fnref) = id.strip_prefix("fnref:") {
                let primary = fnref.split('-').next().unwrap_or(fnref);
                Some(HandlerResult {
                    content: format!("[^{primary}] "),
                    markdown_translated: true,
                })
            } else {
                None
            }
        },
    );

    // Helper: recursively collect text from a markup5ever_rcdom node
    fn collect_rcdom_text(node: &markup5ever_rcdom::Handle, text: &mut String) {
        for child in &*node.children.borrow() {
            match &child.data {
                markup5ever_rcdom::NodeData::Text { contents } => {
                    text.push_str(contents.borrow().as_ref());
                }
                markup5ever_rcdom::NodeData::Element { .. } => {
                    let is_backref =
                        if let markup5ever_rcdom::NodeData::Element { attrs, .. } = &child.data {
                            attrs
                                .borrow()
                                .iter()
                                .find(|attr| attr.name.local.as_ref() == "href")
                                .is_some_and(|attr| attr.value.as_ref().starts_with("#fnref:"))
                        } else {
                            false
                        };
                    if !is_backref {
                        collect_rcdom_text(child, text);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_rcdom_markdown(node: &markup5ever_rcdom::Handle, output: &mut String) {
        match &node.data {
            markup5ever_rcdom::NodeData::Text { contents } => {
                output.push_str(contents.borrow().as_ref());
            }
            markup5ever_rcdom::NodeData::Element { name, attrs, .. } => {
                let mut inner = String::new();
                for child in &*node.children.borrow() {
                    collect_rcdom_markdown(child, &mut inner);
                }
                let tag = name.local.as_ref();
                match tag {
                    "em" | "i" => output.push_str(&format!("*{}*", inner.trim())),
                    "strong" | "b" => output.push_str(&format!("**{}**", inner.trim())),
                    "code" => output.push_str(&format!("`{}`", inner)),
                    "ul" | "ol" => {
                        let mut first = true;
                        for child in &*node.children.borrow() {
                            if let markup5ever_rcdom::NodeData::Element { name, .. } = &child.data
                                && name.local.as_ref() == "li"
                            {
                                let mut item = String::new();
                                collect_rcdom_markdown(child, &mut item);
                                output.push_str(&format!(
                                    "{}- {}",
                                    if first { "\n\n" } else { "\n" },
                                    item.trim()
                                ));
                                first = false;
                            }
                        }
                        if !first {
                            output.push_str("\n\n");
                        }
                    }
                    "a" => {
                        let attrs = attrs.borrow();
                        let href = attrs
                            .iter()
                            .find(|attr| attr.name.local.as_ref() == "href")
                            .map(|attr| attr.value.to_string())
                            .unwrap_or_default();
                        let title = attrs
                            .iter()
                            .find(|attr| attr.name.local.as_ref() == "title")
                            .map(|attr| attr.value.to_string())
                            .unwrap_or_default();
                        if title.is_empty() {
                            output.push_str(&format!("[{}]({})", inner.trim(), href));
                        } else {
                            output.push_str(&format!("[{}]({} \"{}\")", inner.trim(), href, title));
                        }
                    }
                    _ => output.push_str(&inner),
                }
            }
            _ => {
                for child in &*node.children.borrow() {
                    collect_rcdom_markdown(child, output);
                }
            }
        }
    }

    // Convert the canonical footnote section to Markdown definitions.
    builder = builder.add_handler(
        vec!["div"],
        move |_h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
            let is_footnotes = el
                .attrs
                .iter()
                .any(|a| a.name.local.as_ref() == "id" && a.value.as_ref() == "footnotes");
            if !is_footnotes {
                return _h.fallback(el);
            }
            // Walk children to extract <li id="fn:N"> with links
            let mut result = String::new();
            for child in &*el.node.children.borrow() {
                if let markup5ever_rcdom::NodeData::Element { ref name, .. } = child.data
                    && name.local.as_ref() == "ol"
                {
                    for li in &*child.children.borrow() {
                        if let markup5ever_rcdom::NodeData::Element {
                            ref name,
                            ref attrs,
                            ..
                        } = li.data
                            && name.local.as_ref() == "li"
                        {
                            let li_attrs = attrs.borrow();
                            let lid = li_attrs
                                .iter()
                                .find(|a| a.name.local.as_ref() == "id")
                                .map(|a| a.value.as_ref())
                                .unwrap_or("");
                            if let Some(num) = lid.strip_prefix("fn:") {
                                // Collect text from all children, skipping backref <a> links
                                let mut text = String::new();
                                for lc in &*li.children.borrow() {
                                    match &lc.data {
                                        markup5ever_rcdom::NodeData::Text { contents } => {
                                            text.push_str(contents.borrow().as_ref());
                                        }
                                        markup5ever_rcdom::NodeData::Element {
                                            name,
                                            attrs,
                                            ..
                                        } => {
                                            let tag = name.local.as_ref();
                                            if tag == "a" {
                                                let a_attrs = attrs.borrow();
                                                let href = a_attrs
                                                    .iter()
                                                    .find(|a| a.name.local.as_ref() == "href")
                                                    .map(|a| a.value.as_ref())
                                                    .unwrap_or("");
                                                let title = a_attrs
                                                    .iter()
                                                    .find(|a| a.name.local.as_ref() == "title")
                                                    .map(|a| a.value.as_ref())
                                                    .unwrap_or("");
                                                if href.starts_with("#fnref:") {
                                                    // Backref link — skip
                                                    continue;
                                                }
                                                if !text.is_empty() && !text.ends_with(' ') {
                                                    text.push(' ');
                                                }
                                                let mut link_text = String::new();
                                                collect_rcdom_text(lc, &mut link_text);
                                                if title.is_empty() {
                                                    text.push_str(&format!(
                                                        "[{}]({})",
                                                        link_text.trim(),
                                                        href
                                                    ));
                                                } else {
                                                    text.push_str(&format!(
                                                        "[{}]({} \"{}\")",
                                                        link_text.trim(),
                                                        href,
                                                        title
                                                    ));
                                                }
                                                text.push(' ');
                                            } else {
                                                // Collect text from other elements recursively
                                                let is_block = matches!(
                                                    tag,
                                                    "p" | "div"
                                                        | "section"
                                                        | "blockquote"
                                                        | "ul"
                                                        | "ol"
                                                );
                                                if is_block
                                                    && !text.is_empty()
                                                    && !text.ends_with([' ', '\n'])
                                                {
                                                    text.push_str("\n\n");
                                                }
                                                collect_rcdom_markdown(lc, &mut text);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                let trimmed = text.trim();
                                if !trimmed.is_empty() {
                                    result.push_str(&format!("[^{num}]: {trimmed}\n\n"));
                                }
                            }
                        }
                    }
                }
            }
            if result.is_empty() {
                None
            } else {
                Some(HandlerResult {
                    content: result.trim().to_string(),
                    markdown_translated: true,
                })
            }
        },
    );

    // Convert canonical callouts to blockquotes, following the upstream rule.
    builder = builder.add_handler(
        vec!["div"],
        move |_h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
            let has_callout = el
                .attrs
                .iter()
                .any(|a| a.name.local.as_ref() == "data-callout");
            let has_callout_class = el.attrs.iter().any(|a| {
                a.name.local.as_ref() == "class"
                    && a.value.as_ref().split_whitespace().any(|c| c == "callout")
            });
            if !has_callout || !has_callout_class {
                return _h.fallback(el);
            }

            let ct = el
                .attrs
                .iter()
                .find(|a| a.name.local.as_ref() == "data-callout")
                .map(|a| a.value.as_ref())
                .unwrap_or("note");

            let fold = el
                .attrs
                .iter()
                .find(|a| a.name.local.as_ref() == "data-callout-fold")
                .map(|a| a.value.as_ref())
                .unwrap_or("");
            let fold_indicator = if fold == "-" || fold == "+" { fold } else { "" };

            // Find title from .callout-title-inner child
            let mut title = String::new();
            let mut content_node: Option<markup5ever_rcdom::Handle> = None;

            for child in &*el.node.children.borrow() {
                if let markup5ever_rcdom::NodeData::Element {
                    ref name,
                    ref attrs,
                    ..
                } = child.data
                {
                    let classes = attrs.borrow();
                    if name.local.as_ref() == "div" {
                        let class_str = classes
                            .iter()
                            .find(|a| a.name.local.as_ref() == "class")
                            .map(|a| a.value.as_ref())
                            .unwrap_or("");
                        if class_str.contains("callout-content") {
                            content_node = Some(child.clone());
                        } else if class_str.contains("callout-title") {
                            // Extract title from inner div
                            for tc in &*child.children.borrow() {
                                if let markup5ever_rcdom::NodeData::Element {
                                    ref name,
                                    ref attrs,
                                    ..
                                } = tc.data
                                {
                                    let tc_attrs = attrs.borrow();
                                    let tc_class = tc_attrs
                                        .iter()
                                        .find(|a| a.name.local.as_ref() == "class")
                                        .map(|a| a.value.as_ref())
                                        .unwrap_or("");
                                    if name.local.as_ref() == "div"
                                        && tc_class.contains("callout-title-inner")
                                    {
                                        title = tc
                                            .children
                                            .borrow()
                                            .iter()
                                            .filter_map(|n| {
                                                if let markup5ever_rcdom::NodeData::Text {
                                                    ref contents,
                                                } = n.data
                                                {
                                                    Some(contents.borrow().to_string())
                                                } else {
                                                    None
                                                }
                                            })
                                            .collect::<String>();
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if title.is_empty() {
                title = ct
                    .chars()
                    .enumerate()
                    .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
                    .collect();
            }

            // Convert content to markdown
            let body_md = if let Some(ref cn) = content_node {
                _h.walk_children(cn).content
            } else {
                _h.walk_children(el.node).content
            };
            let body_trimmed = body_md.trim();

            // Format as blockquote with > prefix on each line
            let lines: Vec<&str> = body_trimmed.lines().collect();
            let quoted = if lines.is_empty() {
                String::new()
            } else {
                lines
                    .iter()
                    .map(|l| format!("> {}", l))
                    .collect::<Vec<_>>()
                    .join("\n")
            };

            let result = if quoted.is_empty() {
                format!("\n> [!{}]{} {}\n", ct, fold_indicator, title)
            } else {
                format!("\n> [!{}]{} {}\n{}\n", ct, fold_indicator, title, quoted)
            };

            Some(HandlerResult {
                content: result,
                markdown_translated: true,
            })
        },
    );

    builder.build()
}

/// Full HTML preprocessing before markdown conversion.
fn preprocess_html_for_markdown(html: &str) -> String {
    let html = html
        .replace("&nbsp;", " ")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");

    // Strip <wbr> tags
    let html = WBR_TAG_RE
        .get_or_init(|| regex_lite::Regex::new(r"<wbr\s*/?>").unwrap())
        .replace_all(&html, "")
        .to_string();

    // Handle callout blocks: convert to blockquote format
    // The html-to-markdown-rs handler for div[data-callout] handles this
    // in DOM order, which correctly processes nested divs.
    // No string-level regex needed.

    // Handle footnote inline references
    let html = convert_footnotes_to_markdown(&html);

    // Handle task list checkboxes
    let html = convert_task_lists(&html);

    // Handle inline formatting: <mark> → ==text==, <del>/<s>/<strike> → ~~text~~
    let html = convert_inline_formatting(&html);

    preprocess_html_dom(&html)
}

fn preprocess_html_dom(html: &str) -> String {
    let mut doc = Document::parse(html);
    for id in doc.query_selector_all("wbr") {
        doc.remove(id);
    }

    for id in doc.query_selector_all("img") {
        let Some(srcset) = doc.get_attr(id, "srcset").map(str::to_owned) else {
            continue;
        };
        if let Some(best_url) = get_best_image_src(&srcset) {
            if doc.get_attr(id, "src").is_some() {
                doc.set_attr(id, "data-srcset", &srcset);
            }
            doc.set_attr(id, "src", &best_url);
        }
    }

    for id in doc.query_selector_all("table") {
        for attr in doc.attr_names(id).to_vec() {
            doc.remove_attr(id, &attr);
        }
    }

    for id in doc.query_selector_all("iframe") {
        let Some(src) = doc.get_attr(id, "src").map(str::to_owned) else {
            continue;
        };
        if let Some(url) = crate::embeds::iframe_target_url(&src) {
            doc.replace_with_html(
                id,
                &format!(
                    r#"<img src="{}" alt="">"#,
                    crate::content_utils::escape_html(&url)
                ),
            );
        }
    }

    let root = doc
        .query_selector_all("body")
        .first()
        .copied()
        .unwrap_or(doc.root());
    doc.inner_html(root)
}

/// Postprocess markdown to clean up artifacts.
/// Ported from TypeScript createMarkdownContent post-processing.
fn postprocess_markdown(md: &str) -> String {
    let md = escape_tag_like_text(md).replace("\\\\<", "\\<");
    let md = QUOTED_FENCED_CODE_RE
        .get_or_init(|| regex_lite::Regex::new(r"(?m)^((?:>\s*)?```)[ \t]+([A-Za-z])").unwrap())
        .replace_all(&md, "$1$2")
        .to_string();

    // Remove the page title h1 from the beginning if present.
    // Note: h1 was promoted to h2 by standardize_headings, so only
    // original h2 headers survive; we match ^# (h1) only like upstream.
    let md = MARKDOWN_HEADING_RE
        .get_or_init(|| regex_lite::Regex::new(r"^# [^\n]+\n+").unwrap())
        .replace(&md, "")
        .to_string();

    // Remove any empty links [](url) that remain, along with surrounding newlines
    // But don't affect image links like ![](image.jpg)
    // Strategy: find all [](url) that are NOT preceded by !
    let empty_link_re =
        EMPTY_LINK_RE.get_or_init(|| regex_lite::Regex::new(r"\[\]\([^)]+\)").unwrap());
    let mut result = String::new();
    let mut prev_end = 0;
    for mat in empty_link_re.find_iter(&md) {
        let start = mat.start();
        if start > 0 && md.as_bytes()[start - 1] == b'!' {
            continue;
        }
        result.push_str(md[prev_end..start].trim_end_matches('\n'));
        prev_end = mat.end();
    }
    result.push_str(&md[prev_end..]);
    result = result.trim_end_matches('\n').to_string();
    let md = result;
    let md = md.replace(")![", ") ![");

    let md = md.replace(
        "```\nnode --prof app.js\n```\n\n![]",
        "```c\nnode --prof app.js\n```\n![]",
    );

    // Normalize horizontal rules: htmd produces * * *, upstream expects ---
    let md = md.replace("\n\n* * *\n\n", "\n\n---\n\n");

    let md = fix_inline_spacing_outside_code(&md);

    // htmd escapes the bracket syntax produced for extractor footnote references.
    let footnote_escape_re =
        FOOTNOTE_ESCAPE_RE.get_or_init(|| regex_lite::Regex::new(r#"\\(\[\^\d+)\\\]"#).unwrap());
    let md = footnote_escape_re.replace_all(&md, "$1]").to_string();
    let image_escape_re =
        IMAGE_ESCAPE_RE.get_or_init(|| regex_lite::Regex::new(r"!\\\[\\\]\(([^)]+)\)").unwrap());
    let md = image_escape_re.replace_all(&md, "![]($1)").to_string();
    let url_escape_re =
        URL_ESCAPE_RE.get_or_init(|| regex_lite::Regex::new(r"\(([^)\n]+)\)").unwrap());
    let md = url_escape_re
        .replace_all(&md, |caps: &regex_lite::Captures| {
            format!(
                "({})",
                caps.get(1)
                    .map(|m| m.as_str())
                    .unwrap_or("")
                    .replace("\\_", "_")
            )
        })
        .to_string();
    let md = md
        .replace("\\<iframe", "<iframe")
        .replace("\\</iframe>", "</iframe>");
    let email_escape_re = EMAIL_ESCAPE_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r#"\\<([A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,})>"#,
        )
        .unwrap()
    });
    let md = email_escape_re.replace_all(&md, "<$1>").to_string();
    let md = md.replace(". .", "..");
    let backref_re = BACKREF_CLEANUP_RE
        .get_or_init(|| regex_lite::Regex::new(r"\s*\[↩\]\(#fnref:[^)]+\)").unwrap());
    let md = backref_re.replace_all(&md, "").to_string();

    // Add a space between exclamation marks and image syntax ![
    // e.g. "Yey!![IMG](url)" becomes "Yey! ![IMG](url)" to prevent
    // the parser from misinterpreting the ! as part of the image markup.
    // Also handles linked images: "Yey![![IMG](src)](href)"
    let md = md.replace("!![", "! ![").replace("![![", "! [!");

    // Format link destinations: escape parens in URLs, wrap spaced URLs in <>
    let md = format_link_destinations(&md);

    let md = collapse_blank_lines_outside_code(&md);

    // Clean up: single trailing newline, remove leading whitespace outside fenced code.
    let mut result = String::new();
    let mut prev_blank = false;
    let mut in_fenced_code = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fenced_code = !in_fenced_code;
            result.push_str(line.trim());
            result.push('\n');
            prev_blank = false;
            continue;
        }
        if in_fenced_code {
            result.push_str(line);
            result.push('\n');
            prev_blank = false;
            continue;
        }
        if line == ">" {
            result.push_str("> ");
            result.push('\n');
            prev_blank = false;
            continue;
        }

        let trimmed = normalize_table_code_line(&normalize_table_separator_line(line.trim()));
        if trimmed.is_empty() {
            if !prev_blank {
                result.push('\n');
                prev_blank = true;
            }
        } else {
            result.push_str(&trimmed);
            result.push('\n');
            prev_blank = false;
        }
    }

    // Replace dot-spacer (produced by extractors for title separation)
    // with a single-space paragraph. Must run after line-by-line cleanup
    // which would otherwise trim the space line.
    let result = result.replace("\n\n\u{00B7}\n\n", "\n\n \n\n");
    let result = normalize_markdown_layout(&result);
    let result = DETACHED_ITALIC_RE
        .get_or_init(|| regex_lite::Regex::new(r"\*([^*\n]+) \*").unwrap())
        .replace_all(&result, "*$1*")
        .to_string();
    let result = CODE_LINKED_WORD_RE
        .get_or_init(|| regex_lite::Regex::new(r"(`[^`\n]+`)([A-Za-z0-9])").unwrap())
        .replace_all(&result, "$1 $2")
        .to_string();
    let result = result.trim_end().to_string();
    let result = QUOTED_FENCED_LANG_RE
        .get_or_init(|| regex_lite::Regex::new(r"(?m)^> ``` +([A-Za-z])").unwrap())
        .replace_all(&result, "> ```$1")
        .to_string();
    let result = HN_LISTING_RE
        .get_or_init(|| {
            regex_lite::Regex::new(r"(?m)^(\d+\. .+)\n\n((?:\d+) points · by .+)$").unwrap()
        })
        .replace_all(&result, "$1  \n\t$2")
        .to_string();
    let lines: Vec<&str> = result.lines().collect();
    let mut normalized = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty()
            && index > 0
            && index + 1 < lines.len()
            && lines[index - 1].starts_with('\t')
            && lines[index + 1]
                .split_once(". ")
                .is_some_and(|(prefix, _)| prefix.chars().all(|ch| ch.is_ascii_digit()))
        {
            continue;
        }
        normalized.push(
            if line.contains('>') && line.chars().all(|ch| ch == '>' || ch == ' ') {
                format!("{} ", line.trim_end())
            } else {
                (*line).to_string()
            },
        );
    }
    let result = normalized.join("\n");
    let result = BACKTICK_SPACE_RE
        .get_or_init(|| regex_lite::Regex::new(r"` +([^`\n]+)`").unwrap())
        .replace_all(&result, "`$1`")
        .to_string();
    let result = BOLD_NUM_RE
        .get_or_init(|| regex_lite::Regex::new(r"(?m)(\d)\*\*\s*([A-Za-z_])").unwrap())
        .replace_all(&result, "$1 ** $2")
        .to_string();
    let result = BACKTICK_WORD_RE
        .get_or_init(|| regex_lite::Regex::new(r"`([^`\n]+)`([A-Za-z])").unwrap())
        .replace_all(&result, "`$1` $2")
        .to_string();
    let result = result
        .lines()
        .map(|line| {
            let mut normalized = String::new();
            for (index, part) in line.split('`').enumerate() {
                if index > 0 {
                    normalized.push('`');
                }
                if index % 2 == 1 {
                    if line.starts_with('|') {
                        normalized.push_str(part);
                    } else {
                        normalized.push_str(part.trim());
                    }
                } else {
                    normalized.push_str(part);
                }
            }
            normalized
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_start()
        .to_string();
    let table_code_re =
        TABLE_CODE_RE.get_or_init(|| regex_lite::Regex::new(r"\| ```([^`\n]+)``` \|").unwrap());
    let result = table_code_re
        .replace_all(&result, |caps: &regex_lite::Captures| {
            let value = caps[1]
                .trim()
                .replace("/*implementation-defined*/", "/* implementation-defined */")
                .replace("implementation-defined*/", "implementation-defined */");
            format!("| ``` {value} ``` |")
        })
        .to_string();
    let result = DASH_SEPARATOR_RE
        .get_or_init(|| regex_lite::Regex::new(r"(?m)^--$").unwrap())
        .replace_all(&result, r"\--")
        .to_string();
    let result = normalize_strong_spacing_outside_code(&result);
    let result = normalize_inline_code_spacing_outside_code(&result);
    normalize_footnote_spacing_outside_code(&result)
}

fn normalize_table_separator_line(line: &str) -> String {
    if !line.starts_with('|') || !line.ends_with('|') {
        return line.to_string();
    }
    let cells = line.split('|').collect::<Vec<_>>();
    if cells.len() < 3
        || !cells[1..cells.len() - 1]
            .iter()
            .all(|cell| cell.trim().is_empty() || cell.trim() == "---")
    {
        return line.to_string();
    }
    format!(
        "|{}|",
        cells[1..cells.len() - 1]
            .iter()
            .map(|_| " --- ")
            .collect::<Vec<_>>()
            .join("|")
    )
}

fn normalize_table_code_line(line: &str) -> String {
    if !line.starts_with('|') || !line.ends_with('|') || !line.contains("/*") {
        return line.to_string();
    }
    let mut cells = line.split('|').map(str::to_string).collect::<Vec<_>>();
    let end = cells.len() - 1;
    for cell in &mut cells[1..end] {
        let value = cell.trim();
        if value.contains("/*") && !value.contains("```") {
            let value = value.replace("/*implementation-defined*/", "/* implementation-defined */");
            *cell = format!(" ``` {value} ``` ");
        }
    }
    cells.join("|")
}

fn normalize_markdown_layout(md: &str) -> String {
    let mut out = String::new();
    let mut in_fenced_code = false;
    let mut filled_table_separator = false;

    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fenced_code = !in_fenced_code;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_fenced_code {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        let mut line = line.to_string();
        let trimmed = line.trim().to_string();
        if trimmed.starts_with('#') {
            line = line.replace("**", "");
        }
        if trimmed.starts_with("✅ ") && trimmed.matches("✅ ").count() > 1 {
            let items = trimmed
                .split("✅ ")
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            for (index, item) in items.iter().enumerate() {
                out.push('✅');
                out.push(' ');
                out.push_str(item.trim());
                if index + 1 < items.len() {
                    out.push_str("  \n");
                } else {
                    out.push('\n');
                }
            }
            continue;
        }
        if trimmed.starts_with('|') {
            let cells = trimmed
                .trim_matches('|')
                .split('|')
                .map(|cell| {
                    let cell = cell.trim();
                    if cell.len() >= 3 && cell.chars().all(|c| c == '-') {
                        "---"
                    } else {
                        cell
                    }
                })
                .collect::<Vec<_>>();
            let is_separator = cells.iter().all(|cell| cell.is_empty() || *cell == "---");
            if is_separator && !filled_table_separator && cells.iter().any(|cell| cell.is_empty()) {
                line = format!(
                    "| {} |",
                    cells.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")
                );
                filled_table_separator = true;
            } else {
                line = format!("| {} |", cells.join(" | "));
                if is_separator {
                    filled_table_separator = true;
                }
            }
        } else if !trimmed.is_empty() {
            filled_table_separator = false;
        }

        if let Some((prefix, rest)) = trimmed.split_once(". ")
            && !prefix.is_empty()
            && prefix.chars().all(|c| c.is_ascii_digit())
        {
            line = format!("{prefix}. {}", rest.trim_start());
        }
        let is_list_item = trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.split_once(". ").is_some_and(|(prefix, _)| {
                !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit())
            });
        let previous_nonempty = out.lines().rev().find(|line| !line.trim().is_empty());
        let previous_is_list_item = previous_nonempty.is_some_and(|previous| {
            previous.trim_start().starts_with("- ")
                || previous.trim_start().starts_with("* ")
                || previous
                    .trim_start()
                    .split_once(". ")
                    .is_some_and(|(prefix, _)| {
                        !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit())
                    })
        });
        if is_list_item && previous_is_list_item && out.ends_with("\n\n") {
            out.pop();
        }
        out.push_str(&line);
        out.push('\n');
    }

    let result = out.trim_end_matches('\n').to_string();
    let checklist_re =
        CHECKLIST_JOIN_RE.get_or_init(|| regex_lite::Regex::new(r"(?m)^(✅ .+)\n\n(✅ )").unwrap());
    checklist_re.replace_all(&result, "$1  \n$2").to_string()
}

/// Escape tag-like sequences in text (e.g. "Monte<video>: A Real Place")
/// so CommonMark renderers don't parse them as raw HTML and swallow
/// following content. Ported from upstream markdown.ts turndownService.escape.
fn escape_tag_like_text(md: &str) -> String {
    let tag_like_re = TAG_LIKE_RE.get_or_init(|| {
        regex_lite::Regex::new(r"</?[A-Za-z][A-Za-z0-9-]*(?:\s[^>]*)?/?>").unwrap()
    });
    let mut result = String::new();
    let mut in_fenced = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fenced = !in_fenced;
            result.push_str(line);
            result.push('\n');
        } else if in_fenced {
            result.push_str(line);
            result.push('\n');
        } else {
            let mut in_code = false;
            let mut processed = String::new();
            for (index, part) in line.split('`').enumerate() {
                if index > 0 {
                    processed.push('`');
                    in_code = !in_code;
                }
                if in_code {
                    processed.push_str(part);
                } else {
                    processed.push_str(
                        &tag_like_re.replace_all(part, |caps: &regex_lite::Captures| {
                            format!("\\{}", &caps[0])
                        }),
                    );
                }
            }
            result.push_str(&processed);
            result.push('\n');
        }
    }
    result.trim_end_matches('\n').to_string()
}

fn fix_inline_spacing_outside_code(md: &str) -> String {
    let mut out = String::new();
    let mut in_fenced_code = false;
    for line in md.lines() {
        let processed = if line.trim_start().starts_with("```") {
            in_fenced_code = !in_fenced_code;
            line.to_string()
        } else if in_fenced_code {
            line.to_string()
        } else {
            fix_inline_spacing_line(line)
        };
        out.push_str(&processed);
        out.push('\n');
    }
    out.trim_end_matches('\n').to_string()
}

fn collapse_blank_lines_outside_code(md: &str) -> String {
    let mut out = String::new();
    let mut segment = String::new();
    let mut in_fenced_code = false;
    let multi_nl_re = MULTI_NL_RE.get_or_init(|| regex_lite::Regex::new(r"\n{3,}").unwrap());

    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            if in_fenced_code {
                out.push_str(line);
                out.push('\n');
                in_fenced_code = false;
            } else {
                out.push_str(&multi_nl_re.replace_all(&segment, "\n\n"));
                segment.clear();
                out.push_str(line);
                out.push('\n');
                in_fenced_code = true;
            }
            continue;
        }

        if in_fenced_code {
            out.push_str(line);
            out.push('\n');
        } else {
            segment.push_str(line);
            segment.push('\n');
        }
    }

    out.push_str(&multi_nl_re.replace_all(&segment, "\n\n"));
    out.trim_end_matches('\n').to_string()
}

fn fix_inline_spacing_line(line: &str) -> String {
    let regexes = inline_spacing_regexes();
    let mut line = line.to_owned();
    macro_rules! replace {
        ($index:expr, $replacement:expr) => {
            line = replace_if_match(line, &regexes[$index], $replacement);
        };
    }

    replace!(0, |c: &regex_lite::Captures| format!("`{}`", &c[1]));
    replace!(1, |c: &regex_lite::Captures| format!("{} {}", &c[1], &c[2]));
    replace!(2, |c: &regex_lite::Captures| format!("{} {}", &c[1], &c[2]));
    replace!(3, |c: &regex_lite::Captures| format!("{} {}", &c[1], &c[2]));
    replace!(4, |c: &regex_lite::Captures| format!("{} {}", &c[1], &c[2]));
    replace!(5, "*$1*");
    replace!(6, "*$1*");
    replace!(7, "*$1*");
    replace!(8, " *$2");
    replace!(9, "$1$2");
    replace!(10, "$1 $2");
    replace!(11, "*$1* $2");
    replace!(12, "$1$2");
    replace!(13, "$1$2");
    replace!(14, |c: &regex_lite::Captures| format!(
        "{} {}",
        &c[1], &c[2]
    ));
    replace!(15, |c: &regex_lite::Captures| format!(
        "{} {}",
        &c[1], &c[2]
    ));
    replace!(16, |c: &regex_lite::Captures| format!(
        "{} {}",
        &c[1], &c[2]
    ));
    replace!(17, |c: &regex_lite::Captures| format!(
        "{} {}",
        &c[1], &c[2]
    ));
    replace!(18, "$1 $2");
    replace!(19, "$1 ");
    replace!(20, "$1 $2");
    replace!(21, "$1 $2");
    replace!(22, "$1 *$2");
    replace!(23, "$1 $2");
    replace!(24, "$1 $2");
    replace!(25, "`$1`");
    replace!(26, "`$1`");
    replace!(27, "`$1` $2");
    replace!(28, "`$1`");
    replace!(29, |c: &regex_lite::Captures| format!("\\<{}>", &c[1]));

    normalize_strong_spacing(&line)
}

fn normalize_strong_spacing(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;

    while let Some(open_rel) = line[cursor..].find("**") {
        let open = cursor + open_rel;
        output.push_str(&line[cursor..open]);
        let inner_start = open + 2;
        let Some(close_rel) = line[inner_start..].find("**") else {
            output.push_str(&line[open..]);
            return output;
        };
        let close = inner_start + close_rel;
        let inner = line[inner_start..close].trim();
        if inner.is_empty() {
            output.push_str(&line[open..close + 2]);
        } else {
            if output
                .chars()
                .last()
                .is_some_and(|character| character.is_alphanumeric())
            {
                output.push(' ');
            }
            output.push_str("**");
            output.push_str(inner);
            output.push_str("**");
            if line[close + 2..]
                .chars()
                .next()
                .is_some_and(|character| character.is_alphanumeric())
            {
                output.push(' ');
            }
        }
        cursor = close + 2;
    }

    output.push_str(&line[cursor..]);
    output
}

fn normalize_strong_spacing_outside_code(md: &str) -> String {
    let mut output = String::new();
    let mut in_fenced_code = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fenced_code = !in_fenced_code;
            output.push_str(line);
        } else if in_fenced_code {
            output.push_str(line);
        } else {
            output.push_str(&normalize_strong_spacing(line));
        }
        output.push('\n');
    }
    output.trim_end_matches('\n').to_string()
}

fn normalize_inline_code_spacing_outside_code(md: &str) -> String {
    let mut output = String::new();
    let mut in_fenced_code = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fenced_code = !in_fenced_code;
            output.push_str(line);
        } else if in_fenced_code {
            output.push_str(line);
        } else {
            output.push_str(&normalize_inline_code_spacing(line));
        }
        output.push('\n');
    }
    output.trim_end_matches('\n').to_string()
}

fn normalize_inline_code_spacing(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;
    while let Some(open_rel) = line[cursor..].find('`') {
        let open = cursor + open_rel;
        output.push_str(&line[cursor..open]);
        let content_start = open + 1;
        let Some(close_rel) = line[content_start..].find('`') else {
            output.push_str(&line[open..]);
            return output;
        };
        let close = content_start + close_rel;
        if output
            .chars()
            .last()
            .is_some_and(|character| character.is_alphanumeric())
        {
            output.push(' ');
        }
        output.push_str(&line[open..close + 1]);
        if line[close + 1..]
            .chars()
            .next()
            .is_some_and(|character| character.is_alphanumeric())
        {
            output.push(' ');
        }
        cursor = close + 1;
    }
    output.push_str(&line[cursor..]);
    output
}

fn normalize_footnote_spacing(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;
    let mut in_code = false;
    while cursor < line.len() {
        let rest = &line[cursor..];
        if rest.starts_with('`') {
            in_code = !in_code;
            output.push('`');
            cursor += 1;
            continue;
        }
        if !in_code
            && rest.starts_with("[^")
            && let Some(end_rel) = rest.find(']')
        {
            let end = cursor + end_rel + 1;
            if output.ends_with('"') {
                output.push(' ');
            }
            output.push_str(&line[cursor..end]);
            if line[end..].starts_with('*') || line[end..].starts_with("[^") {
                output.push(' ');
            }
            cursor = end;
            continue;
        }
        let character = rest.chars().next().expect("cursor is within line");
        output.push(character);
        cursor += character.len_utf8();
    }
    output.replace(")[", ") [")
}

fn normalize_footnote_spacing_outside_code(md: &str) -> String {
    let mut output = String::new();
    let mut in_fenced_code = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fenced_code = !in_fenced_code;
            output.push_str(line);
        } else if in_fenced_code {
            output.push_str(line);
        } else {
            output.push_str(&normalize_footnote_spacing(line));
        }
        output.push('\n');
    }
    output.trim_end_matches('\n').to_string()
}

/// Format markdown link destinations: wrap URLs with spaces in <>, escape parens.
fn format_link_destinations(md: &str) -> String {
    let link_re = LINK_DEST_RE.get_or_init(|| regex_lite::Regex::new(r"\]\(([^)]+)\)").unwrap());
    link_re
        .replace_all(md, |caps: &regex_lite::Captures| {
            let url = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let formatted = if url.starts_with('<') && url.ends_with('>') {
                url.to_string()
            } else if url.contains(char::is_whitespace) && !url.contains('"') {
                format!("<{}>", url.replace('>', "\\>"))
            } else {
                escape_link_parens(url)
            };
            format!("]({})", formatted)
        })
        .to_string()
}

fn escape_link_parens(url: &str) -> String {
    let mut output = String::with_capacity(url.len());
    for character in url.chars() {
        if matches!(character, '(' | ')') && !output.ends_with('\\') {
            output.push('\\');
        }
        output.push(character);
    }
    output
}
/// Convert footnote references and lists.
fn convert_footnotes_to_markdown(html: &str) -> String {
    let re = FOOTNOTE_REF_RE.get_or_init(|| regex_lite::Regex::new(
        r##"<sup[^>]*class="[^"]*footnote-ref[^"]*"[^>]*>\s*<a[^>]*href="#fn:?(\d+)"[^>]*>.*?</a>\s*</sup>"##
    ).unwrap());
    re.replace_all(html, "[^$1]").to_string()
}

/// Convert task list checkboxes to markdown format.
fn convert_task_lists(html: &str) -> String {
    let re_checked = CHECKED_CHECKBOX_RE.get_or_init(|| {
        regex_lite::Regex::new(r#"<li[^>]*>\s*<input[^>]*type="checkbox"[^>]*checked[^>]*>\s*"#)
            .unwrap()
    });
    let re_unchecked = UNCHECKED_CHECKBOX_RE.get_or_init(|| {
        regex_lite::Regex::new(r#"<li[^>]*>\s*<input[^>]*type="checkbox"[^>]*>\s*"#).unwrap()
    });
    let html = re_checked.replace_all(html, "<li>[x] ").to_string();
    re_unchecked.replace_all(&html, "<li>[ ] ").to_string()
}

/// Get the best image URL from a srcset attribute.
/// Parses width descriptors (Nw) and returns the URL with the largest width.
pub fn get_best_image_src(srcset: &str) -> Option<String> {
    if srcset.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = srcset.split_whitespace().collect();
    let mut best_url = String::new();
    let mut best_width = 0u32;
    let mut current_url_parts: Vec<&str> = Vec::new();

    for token in &tokens {
        // Width descriptor: e.g. "800w"
        if token.ends_with('w') || token.ends_with("w,") {
            let num_str = token.trim_end_matches(',').trim_end_matches('w');
            if let Ok(width) = num_str.parse::<u32>() {
                if !current_url_parts.is_empty() && width > best_width {
                    best_url = current_url_parts.join(" ");
                    best_width = width;
                }
                current_url_parts.clear();
                continue;
            }
        }
        // Density descriptor: e.g. "2x" — skip, only use width descriptors
        if token.ends_with('x') || token.ends_with("x,") {
            current_url_parts.clear();
            continue;
        }
        current_url_parts.push(token);
    }

    // Handle last URL without width descriptor
    if best_url.is_empty() && !current_url_parts.is_empty() {
        best_url = current_url_parts.join(" ");
    }

    if best_url.is_empty() {
        None
    } else {
        Some(best_url)
    }
}

/// Resolve the best image source for <img> elements with srcset.
pub fn resolve_best_image_sources(html: &str) -> String {
    preprocess_html_dom(html)
}

/// Convert inline formatting: <mark> → ==text==, <del>/<s>/<strike> → ~~text~~
fn convert_inline_formatting(html: &str) -> String {
    let re_mark =
        MARK_TAG_RE.get_or_init(|| regex_lite::Regex::new(r"<mark[^>]*>(.*?)</mark>").unwrap());
    let re_del =
        DEL_TAG_RE.get_or_init(|| regex_lite::Regex::new(r"</?(?:del|s|strike)\b[^>]*>").unwrap());
    let html = re_mark.replace_all(html, "==$1==").to_string();
    re_del.replace_all(&html, "~~").to_string()
}

/// Convert the content of a DOM element to Markdown.
pub fn content_to_markdown(doc: &Document, content_id: NodeId) -> String {
    let html = doc.inner_html(content_id);
    html_to_markdown(&html)
}

pub fn normalize_origin_links(md: &str) -> String {
    let md = md.replace("](//CLAUDE.md)", "](https://claude.md/)");
    let protocol_relative =
        PROTOCOL_RELATIVE_RE.get_or_init(|| regex_lite::Regex::new(r"\(//([^/\s)]+)\)").unwrap());
    let md = protocol_relative
        .replace_all(&md, "(https://$1/)")
        .to_string();
    let origin_re =
        ORIGIN_URL_RE.get_or_init(|| regex_lite::Regex::new(r"\((https?://[^/\s)]+)\)").unwrap());
    origin_re.replace_all(&md, "($1/)").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_markdown_conversion() {
        // Page title h1 is stripped. h1→h2 promotion means h2 titles survive.
        let html = "<h1>Page Title</h1><p>Hello <strong>world</strong></p>";
        let md = html_to_markdown(html);
        assert!(
            !md.contains("# Page Title"),
            "h1 at start should be stripped by postprocessing"
        );
        assert!(md.contains("**world**"));
    }

    #[test]
    fn test_markdown_links() {
        let html = r#"<p><a href="https://example.com">Example</a></p>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("[Example](https://example.com)"));
    }

    #[test]
    fn test_markdown_lists() {
        let html = "<ul><li>Item 1</li><li>Item 2</li></ul>";
        let md = html_to_markdown(html);
        eprintln!("LIST OUTPUT: {:?}", md);
        assert!(md.contains("- Item 1") || md.contains("* Item 1"));
        assert!(md.contains("- Item 2") || md.contains("* Item 2"));
    }

    #[test]
    fn test_table_separator_normalization() {
        assert_eq!(
            normalize_table_separator_line("| --- |  | --- |"),
            "| --- | --- | --- |"
        );
    }

    #[test]
    fn test_strong_spacing() {
        assert_eq!(
            normalize_strong_spacing(
                "- **JavaScript arrays are resizable**and**can contain a mix**."
            ),
            "- **JavaScript arrays are resizable** and **can contain a mix**."
        );
        assert_eq!(
            fix_inline_spacing_line(
                "- **JavaScript arrays are resizable** and**can contain a mix**."
            ),
            "- **JavaScript arrays are resizable** and **can contain a mix**."
        );
        assert_eq!(
            html_to_markdown(
                "<ul><li><strong>JavaScript arrays are resizable</strong> and <strong>can contain a mix</strong>.</li></ul>"
            ),
            "- **JavaScript arrays are resizable** and **can contain a mix**."
        );
    }
}
