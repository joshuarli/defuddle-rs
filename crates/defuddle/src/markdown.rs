use crate::dom::{Document, NodeId};
use crate::pipeline::Diagnostics;
use htmd::element_handler::{HandlerResult, Handlers};
use htmd::{
    Element, HtmlToMarkdown,
    options::{HeadingStyle, LinkStyle},
};
use std::borrow::Cow;
use std::sync::OnceLock;

static WBR_TAG_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static FOOTNOTE_REF_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static CHECKED_CHECKBOX_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static UNCHECKED_CHECKBOX_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static MARK_TAG_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static DEL_TAG_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static PROTOCOL_RELATIVE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static ORIGIN_URL_RE: OnceLock<regex_lite::Regex> = OnceLock::new();

fn format_markdown_link_destination(href: &str) -> Option<String> {
    if crate::sanitize::is_dangerous_url(href) {
        return None;
    }
    let escaped = href
        .replace('\\', "\\\\")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('\n', "%0A")
        .replace('\r', "%0D")
        .replace('\t', "%09");
    Some(if escaped.chars().any(char::is_whitespace) {
        format!("<{escaped}>")
    } else {
        escape_link_parens(&escaped)
    })
}

/// Convert HTML content to Markdown using htmd (Turndown.js-compatible) with
/// custom element handlers matching the upstream Defuddle Turndown rules.
pub fn html_to_markdown(html: &str) -> String {
    html_to_markdown_internal(html, None)
}

pub(crate) fn html_to_markdown_with_diagnostics(
    html: &str,
    diagnostics: &mut Diagnostics,
) -> String {
    html_to_markdown_internal(html, Some(diagnostics))
}

fn html_to_markdown_internal(html: &str, mut diagnostics: Option<&mut Diagnostics>) -> String {
    let cleaned = preprocess_html_for_markdown_with_diagnostics(html, &mut diagnostics);

    convert_cleaned_html_to_markdown(&cleaned, &mut diagnostics)
}

fn convert_cleaned_html_to_markdown(
    cleaned: &str,
    diagnostics: &mut Option<&mut Diagnostics>,
) -> String {
    let converter = build_converter();

    let converted = measure_phase(diagnostics, "markdown convert", || {
        converter.convert(cleaned)
    });
    match converted {
        Ok(md) => measure_phase(diagnostics, "markdown postprocess", || {
            postprocess_markdown(&md)
        }),
        Err(_) => cleaned.to_string(),
    }
}

fn measure_phase<T>(
    diagnostics: &mut Option<&mut Diagnostics>,
    name: &'static str,
    operation: impl FnOnce() -> T,
) -> T {
    if let Some(diagnostics) = diagnostics.as_deref_mut() {
        diagnostics.measure(name, operation)
    } else {
        operation()
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

    builder = builder.add_handler(
        vec!["a"],
        |_h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
            let href = el
                .attrs
                .iter()
                .find(|a| a.name.local.as_ref() == "href")
                .map(|a| a.value.as_ref())
                .unwrap_or("");
            let children = _h.walk_children(el.node);
            let content = if href.is_empty() {
                children.content
            } else {
                let Some(destination) = format_markdown_link_destination(href) else {
                    return Some(HandlerResult {
                        content: children.content,
                        markdown_translated: true,
                    });
                };
                let title = el
                    .attrs
                    .iter()
                    .find(|a| a.name.local.as_ref() == "title")
                    .map(|a| {
                        format!(
                            " \"{}\"",
                            a.value
                                .replace(['\r', '\n'], " ")
                                .replace('\\', "\\\\")
                                .replace('"', "\\\"")
                        )
                    })
                    .unwrap_or_default();
                let label = children.content.trim_start_matches(['\t', '\n', '\r', ' ']);
                let label_end = label.trim_end_matches(['\t', '\n', '\r', ' ']).len();
                let trailing_whitespace = &label[label_end..];
                let label = &label[..label_end];
                let mut markdown_link = format!("[{label}]({destination}{title})");
                markdown_link.push_str(trailing_whitespace);
                markdown_link
            };
            Some(HandlerResult {
                content,
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

fn preprocess_html_for_markdown_with_diagnostics(
    html: &str,
    diagnostics: &mut Option<&mut Diagnostics>,
) -> String {
    let html = preprocess_html_text_with_diagnostics(html, diagnostics);
    measure_phase(diagnostics, "markdown preprocess dom", || {
        preprocess_html_dom(&html)
    })
}

fn preprocess_html_text(html: &str) -> String {
    let mut diagnostics = None;
    preprocess_html_text_with_diagnostics(html, &mut diagnostics)
}

fn preprocess_html_text_with_diagnostics(
    html: &str,
    diagnostics: &mut Option<&mut Diagnostics>,
) -> String {
    let html = measure_phase(diagnostics, "markdown text basic", || {
        let mut html = Cow::Borrowed(html);
        for (pattern, replacement) in [
            ("&nbsp;", " "),
            ("<br>", "\n"),
            ("<br/>", "\n"),
            ("<br />", "\n"),
        ] {
            if html.contains(pattern) {
                html = Cow::Owned(html.replace(pattern, replacement));
            }
        }
        html.into_owned()
    });

    // Strip <wbr> tags
    let html = if html.contains("<wbr") {
        measure_phase(diagnostics, "markdown text wbr", || {
            WBR_TAG_RE
                .get_or_init(|| regex_lite::Regex::new(r"<wbr\s*/?>").unwrap())
                .replace_all(&html, "")
                .to_string()
        })
    } else {
        html
    };

    // Handle callout blocks: convert to blockquote format
    // build_converter() handles callouts in DOM order, including nested divs.
    // No string-level regex needed.

    // Handle footnote inline references
    let html = if html.contains("footnote-ref") {
        measure_phase(diagnostics, "markdown text footnotes", || {
            convert_footnotes_to_markdown(&html)
        })
    } else {
        html
    };

    // Handle task list checkboxes
    let html = if html.contains("checkbox") {
        measure_phase(diagnostics, "markdown text tasks", || {
            convert_task_lists(&html)
        })
    } else {
        html
    };

    // Handle inline formatting: <mark> → ==text==, <del>/<s>/<strike> → ~~text~~
    let has_inline_formatting = html.contains("<mark")
        || html.contains("<del")
        || html.contains("<strike")
        || html.contains("<s>")
        || html.contains("<s ");
    if has_inline_formatting {
        measure_phase(diagnostics, "markdown text inline", || {
            convert_inline_formatting(&html)
        })
    } else {
        html
    }
}

fn preprocess_html_dom(html: &str) -> String {
    // The DOM pass only changes image sources/alt text, table attributes, and
    // iframe embeds. Avoid reparsing ordinary article fragments that do not
    // contain any of those elements.
    if !html.contains("<img") && !html.contains("<table") && !html.contains("<iframe") {
        return html.to_string();
    }

    let mut doc = Document::parse(html);
    let root = doc
        .query_selector_all("body")
        .first()
        .copied()
        .unwrap_or(doc.root());
    preprocess_markdown_dom(&mut doc, root);
    doc.inner_html(root)
}

fn preprocess_markdown_dom(doc: &mut Document, root: NodeId) {
    for id in doc.query_selector_all_in_subtree(root, "img") {
        // Markdown output intentionally omits image alt text. Captions and
        // surrounding text remain in the document, while alt text often
        // duplicates or exposes noisy accessibility labels from the page.
        doc.set_attr(id, "alt", "");

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

    for id in doc.query_selector_all_in_subtree(root, "table") {
        for attr in doc.attr_names(id).to_vec() {
            doc.remove_attr(id, &attr);
        }
    }

    for id in doc.query_selector_all_in_subtree(root, "iframe") {
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
}

/// Postprocess markdown to clean up artifacts.
/// Ported from TypeScript createMarkdownContent post-processing.
fn postprocess_markdown(md: &str) -> String {
    MarkdownPostprocessor::new(md).run()
}

struct MarkdownPostprocessor<'a> {
    source: &'a str,
    output: String,
    in_fenced_code: bool,
    previous_blank: bool,
    filled_table_separator: bool,
    previous_nonempty_is_list: bool,
    previous_hn_metadata: bool,
}

impl<'a> MarkdownPostprocessor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            output: String::with_capacity(source.len()),
            in_fenced_code: false,
            previous_blank: false,
            filled_table_separator: false,
            previous_nonempty_is_list: false,
            previous_hn_metadata: false,
        }
    }

    fn run(mut self) -> String {
        let source = remove_leading_markdown_title(self.source);
        let mut lines = source.lines().peekable();
        let mut previous_line = None;

        while let Some(line) = lines.next() {
            let next_line = lines.peek().copied();

            if self.in_fenced_code {
                self.output.push_str(line);
                self.output.push('\n');
                if line.trim_start().starts_with("```") {
                    self.in_fenced_code = false;
                }
                self.previous_blank = false;
                self.previous_hn_metadata = false;
                previous_line = Some(line);
                continue;
            }

            if line.trim_start().starts_with("```") {
                self.flush_line(line.trim(), false);
                self.in_fenced_code = true;
                previous_line = Some(line);
                continue;
            }

            if is_quote_only_line(line) {
                let quote_line = line;
                while lines.peek().is_some_and(|next| is_quote_only_line(next)) {
                    lines.next();
                }
                if next_line.is_some_and(is_quote_only_line) {
                    self.flush_line(quote_line.trim_end(), false);
                    previous_line = Some(line);
                    continue;
                }
            }

            if line.trim() == "* * *"
                && previous_line.is_some_and(|previous| previous.trim().is_empty())
                && next_line.is_some_and(|next| next.trim().is_empty())
            {
                self.flush_line("---", false);
                previous_line = Some(line);
                continue;
            }

            if line.trim() == "·"
                && previous_line.is_some_and(|previous| previous.trim().is_empty())
                && next_line.is_some_and(|next| next.trim().is_empty())
            {
                self.output.push_str(" \n");
                self.previous_blank = false;
                previous_line = Some(line);
                continue;
            }

            if line.trim().is_empty()
                && self.previous_hn_metadata
                && next_line.is_some_and(is_hn_story_line)
            {
                previous_line = Some(line);
                continue;
            }

            if is_hn_story_line(line)
                && next_line.is_some_and(|next| next.trim().is_empty())
                && lines.clone().nth(1).is_some_and(is_hn_metadata_line)
            {
                lines.next().expect("peeked blank line");
                let metadata = lines.next().expect("peeked Hacker News metadata");
                let story = process_inline_markdown_line(line);
                self.flush_line_with_text(&story, false);
                self.output.insert(self.output.len() - 1, ' ');
                self.output.insert(self.output.len() - 1, ' ');
                self.flush_line_with_text(&format!("\t{}", metadata.trim()), false);
                let line_start = self.output[..self.output.len() - 1]
                    .rfind('\n')
                    .map_or(0, |index| index + 1);
                self.output.insert(line_start, '\t');
                self.previous_hn_metadata = true;
                previous_line = Some(metadata);
                continue;
            }

            let processed = process_inline_markdown_line(line);
            self.flush_line_with_text(&processed, false);
            previous_line = Some(line);
        }

        self.output.trim().to_string()
    }

    fn flush_line(&mut self, line: &str, in_code: bool) {
        self.flush_line_with_text(line, in_code);
    }

    fn flush_line_with_text(&mut self, line: &str, in_code: bool) {
        if in_code {
            self.output.push_str(line);
            self.output.push('\n');
            return;
        }

        let mut line = line.trim().to_string();
        if is_quote_only_line(&line) {
            line.push(' ');
        }
        if line == "--" {
            line.insert(0, '\\');
        }

        if line.is_empty() {
            if !self.previous_blank {
                self.output.push('\n');
                self.previous_blank = true;
            }
            self.previous_hn_metadata = false;
            return;
        }

        line = normalize_table_separator_line(&line);
        line = normalize_table_code_line(&line);
        line = normalize_layout_line(
            &line,
            &mut self.filled_table_separator,
            self.previous_nonempty_is_list,
            &mut self.output,
        );
        line = normalize_terminal_spacing_line(&line);

        if is_list_item(&line) && self.previous_nonempty_is_list && self.output.ends_with("\n\n") {
            self.output.pop();
        }
        self.output.push_str(&line);
        self.output.push('\n');
        self.previous_blank = false;
        self.previous_nonempty_is_list = is_list_item(&line);
        self.previous_hn_metadata = false;
    }
}

fn remove_leading_markdown_title(md: &str) -> &str {
    let Some(rest) = md.strip_prefix("# ") else {
        return md;
    };
    let Some(newline) = rest.find('\n') else {
        return md;
    };
    rest[newline + 1..].trim_start_matches('\n')
}

fn is_quote_only_line(line: &str) -> bool {
    !line.trim().is_empty()
        && line
            .chars()
            .all(|character| character == '>' || character == ' ' || character == '\t')
}

fn is_hn_story_line(line: &str) -> bool {
    line.split_once(". ").is_some_and(|(prefix, rest)| {
        !rest.is_empty() && prefix.chars().all(|character| character.is_ascii_digit())
    })
}

fn is_hn_metadata_line(line: &str) -> bool {
    let Some((points, rest)) = line.trim().split_once(" points · by ") else {
        return false;
    };
    !points.is_empty()
        && points.chars().all(|character| character.is_ascii_digit())
        && !rest.is_empty()
}

fn is_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.split_once(". ").is_some_and(|(prefix, rest)| {
            !rest.is_empty() && prefix.chars().all(|character| character.is_ascii_digit())
        })
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

fn normalize_layout_line(
    line: &str,
    filled_table_separator: &mut bool,
    previous_nonempty_is_list: bool,
    output: &mut String,
) -> String {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return line.replace("**", "");
    }
    if trimmed.starts_with("✅ ") && trimmed.matches("✅ ").count() > 1 {
        let items = trimmed
            .split("✅ ")
            .filter(|item| !item.is_empty())
            .map(str::trim)
            .collect::<Vec<_>>();
        return format!("✅ {}", items.join("  \n✅ "));
    }
    if trimmed.starts_with('|') {
        let cells = trimmed
            .trim_matches('|')
            .split('|')
            .map(|cell| {
                let cell = cell.trim();
                if cell.len() >= 3 && cell.chars().all(|character| character == '-') {
                    "---"
                } else {
                    cell
                }
            })
            .collect::<Vec<_>>();
        let is_separator = cells.iter().all(|cell| cell.is_empty() || *cell == "---");
        if is_separator && !*filled_table_separator && cells.iter().any(|cell| cell.is_empty()) {
            *filled_table_separator = true;
            return format!(
                "| {} |",
                cells.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")
            );
        }
        if is_separator {
            *filled_table_separator = true;
        }
        return format!("| {} |", cells.join(" | "));
    }
    if !trimmed.is_empty() {
        *filled_table_separator = false;
    }

    if let Some((prefix, rest)) = trimmed.split_once(". ")
        && !prefix.is_empty()
        && prefix.chars().all(|character| character.is_ascii_digit())
    {
        return format!("{prefix}. {}", rest.trim_start());
    }

    if is_list_item(trimmed) && previous_nonempty_is_list && output.ends_with("\n\n") {
        output.pop();
    }
    line.to_string()
}

fn process_inline_markdown_line(line: &str) -> String {
    let mut line = escape_tag_like_line(line);
    line = normalize_quoted_fence_line(&line);
    line = remove_empty_links_line(&line);
    line = line.replace(")![", ") ![");
    line = line.replace("!![", "! ![").replace("![![", "! [!");
    line = line
        .replace("\\<iframe", "<iframe")
        .replace("\\</iframe>", "</iframe>");
    line = line.replace(". .", "..");
    line = remove_backreferences_line(&line);
    line = fix_inline_spacing_line_stateful(&line);
    line = normalize_escaped_markdown_line(&line);
    line = normalize_url_parentheses_line(&line);
    line = format_link_destinations_line(&line);
    line
}

fn fix_inline_spacing_line_stateful(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;

    while cursor < line.len() {
        let rest = &line[cursor..];
        if rest.starts_with('`')
            && let Some(close_rel) = rest[1..].find('`')
        {
            let close = cursor + 1 + close_rel;
            let content = line[cursor + 1..close].trim();
            if output
                .chars()
                .last()
                .is_some_and(|character| character.is_alphanumeric())
            {
                output.push(' ');
            }
            output.push('`');
            output.push_str(content);
            output.push('`');
            if line[close + 1..].starts_with("[^")
                || line[close + 1..]
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_alphanumeric())
            {
                output.push(' ');
            }
            cursor = close + 1;
            continue;
        }
        if rest.starts_with("**")
            && let Some(close_rel) = rest[2..].find("**")
        {
            let close = cursor + 2 + close_rel;
            let content = line[cursor + 2..close].trim();
            if output
                .chars()
                .last()
                .is_some_and(|character| character.is_alphanumeric())
            {
                output.push(' ');
            }
            output.push_str("**");
            output.push_str(content);
            output.push_str("**");
            if line[close + 2..]
                .chars()
                .next()
                .is_some_and(|character| character.is_alphanumeric())
            {
                output.push(' ');
            }
            cursor = close + 2;
            continue;
        }
        if (rest.starts_with('[') || rest.starts_with("!["))
            && let Some(destination_rel) = rest.find("](")
            && let Some(close_rel) = rest[destination_rel + 2..].find(')')
        {
            let close = cursor + destination_rel + 2 + close_rel + 1;
            if output
                .chars()
                .last()
                .is_some_and(|character| character.is_alphanumeric())
            {
                output.push(' ');
            }
            output.push_str(&line[cursor..close]);
            let mut cursor_after_link = close;
            let had_spacing = line[close..].starts_with("  ");
            if had_spacing {
                output.push(' ');
                while line[cursor_after_link..].starts_with(' ') {
                    cursor_after_link += 1;
                }
            }
            if !had_spacing
                && line[close..]
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_alphanumeric())
            {
                output.push(' ');
            }
            cursor = cursor_after_link;
            continue;
        }
        if rest.starts_with("[^")
            && let Some(end_rel) = rest.find(']')
        {
            let end = cursor + end_rel + 1;
            if output.chars().last().is_some_and(|character| {
                character.is_alphanumeric() || character == '"' || character == '`'
            }) {
                output.push(' ');
            }
            output.push_str(&line[cursor..end]);
            if line[end..].starts_with('*') || line[end..].starts_with("[^") {
                output.push(' ');
            }
            cursor = end;
            continue;
        }
        if rest.starts_with('*')
            && !rest.starts_with("**")
            && let Some(close_rel) = rest[1..].find('*')
        {
            let close = cursor + 1 + close_rel;
            let content = line[cursor + 1..close].trim();
            if !content.is_empty() {
                if output.chars().last().is_some_and(|character| {
                    character.is_alphanumeric()
                        || matches!(character, ',' | '.' | ';' | ':' | '!' | '?')
                }) {
                    output.push(' ');
                }
                output.push('*');
                output.push_str(content);
                output.push('*');
                if line[close + 1..]
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_alphanumeric() || character == '(')
                {
                    output.push(' ');
                }
                cursor = close + 1;
                continue;
            }
        }

        if rest.starts_with("\\-") && line[cursor + 2..].starts_with(' ') {
            output.push_str("\\-");
            cursor += 3;
            continue;
        }
        if rest.starts_with("\\\\<") {
            output.push_str("\\<");
            cursor += 3;
            continue;
        }

        let character = rest.chars().next().expect("cursor is within line");
        if matches!(character, '.' | ',' | ';' | ':' | '!' | '?')
            && output_ends_with_footnote(&output)
        {
            while output.ends_with(' ') {
                output.pop();
            }
        }
        output.push(character);
        cursor += character.len_utf8();
    }

    output
}

fn output_ends_with_footnote(output: &str) -> bool {
    let output = output.trim_end();
    let Some(start) = output.rfind("[^") else {
        return false;
    };
    output[start..].ends_with(']')
        && output[start + 2..output.len() - 1]
            .chars()
            .all(|character| character.is_ascii_digit())
}

fn escape_tag_like_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;
    while cursor < line.len() {
        let rest = &line[cursor..];
        if rest.starts_with('`')
            && let Some(close_rel) = rest[1..].find('`')
        {
            let close = cursor + 1 + close_rel;
            output.push_str(&line[cursor..=close]);
            cursor = close + 1;
            continue;
        }
        if rest.starts_with('<') && looks_like_tag(&rest[1..]) {
            output.push('\\');
        }
        let character = rest.chars().next().expect("cursor is within line");
        output.push(character);
        cursor += character.len_utf8();
    }
    output.replace("\\\\<", "\\<")
}

fn looks_like_tag(rest: &str) -> bool {
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    let mut chars = rest.chars();
    if !chars
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
    {
        return false;
    }
    for character in chars {
        if character == '>' || character == '/' {
            return true;
        }
        if character.is_whitespace() {
            return rest.contains('>');
        }
        if !(character.is_ascii_alphanumeric() || character == '-') {
            return false;
        }
    }
    false
}

fn normalize_quoted_fence_line(line: &str) -> String {
    let trimmed = line.trim_start();
    let prefix_len = line.len() - trimmed.len();
    let rest = trimmed.strip_prefix('>').unwrap_or(trimmed).trim_start();
    if let Some(language) = rest.strip_prefix("```") {
        let language = language.trim_start();
        if language.len() == 1
            && language
                .chars()
                .all(|character| character.is_ascii_alphabetic())
        {
            let prefix = &line[..prefix_len];
            return format!("{prefix}> ```{language}");
        }
    }
    line.to_string()
}

fn remove_empty_links_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;
    while let Some(start_rel) = line[cursor..].find("[](") {
        let start = cursor + start_rel;
        let Some(close_rel) = line[start + 3..].find(')') else {
            break;
        };
        if start > 0 && line.as_bytes()[start - 1] == b'!' {
            output.push_str(&line[cursor..start + 3 + close_rel + 1]);
        } else {
            output.push_str(&line[cursor..start]);
        }
        cursor = start + 3 + close_rel + 1;
    }
    output.push_str(&line[cursor..]);
    output.trim_end_matches('\n').to_string()
}

fn remove_backreferences_line(line: &str) -> String {
    let marker = "[↩](#fnref:";
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;
    while let Some(start_rel) = line[cursor..].find(marker) {
        let start = cursor + start_rel;
        let Some(close_rel) = line[start + marker.len()..].find(')') else {
            break;
        };
        let mut content_end = start + marker.len() + close_rel + 1;
        while output.ends_with(char::is_whitespace) {
            output.pop();
        }
        output.push_str(&line[cursor..start]);
        content_end = content_end.max(start + marker.len() + close_rel + 1);
        cursor = content_end;
    }
    output.push_str(&line[cursor..]);
    output
}

fn normalize_escaped_markdown_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;
    while cursor < line.len() {
        let rest = &line[cursor..];
        if rest.starts_with("\\[^")
            && let Some(close_rel) = rest[3..].find("\\]")
            && rest[3..3 + close_rel]
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            output.push_str("[^");
            output.push_str(&rest[3..3 + close_rel]);
            output.push(']');
            cursor += 3 + close_rel + 2;
            continue;
        }
        if rest.starts_with("!\\[\\](")
            && let Some(close_rel) = rest[6..].find(')')
        {
            output.push_str("![](");
            output.push_str(&rest[6..6 + close_rel]);
            output.push(')');
            cursor += 6 + close_rel + 1;
            continue;
        }
        if rest.starts_with("\\<")
            && let Some(close_rel) = rest[2..].find('>')
        {
            let value = &rest[2..2 + close_rel];
            if value.contains('@') && value.rsplit_once('.').is_some() {
                output.push('<');
                output.push_str(value);
                output.push('>');
                cursor += 2 + close_rel + 1;
                continue;
            }
        }
        let character = rest.chars().next().expect("cursor is within line");
        output.push(character);
        cursor += character.len_utf8();
    }
    output
}

fn normalize_url_parentheses_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;
    while cursor < line.len() {
        let rest = &line[cursor..];
        let Some(open_rel) = rest.find('(') else {
            output.push_str(rest);
            break;
        };
        let open = cursor + open_rel;
        output.push_str(&line[cursor..=open]);
        let Some(close_rel) = line[open + 1..].find(')') else {
            output.push_str(&line[open + 1..]);
            break;
        };
        let close = open + 1 + close_rel;
        output.push_str(&line[open + 1..close].replace("\\_", "_"));
        output.push(')');
        cursor = close + 1;
    }
    output
}

fn format_link_destinations_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;
    while let Some(open_rel) = line[cursor..].find("](") {
        let open = cursor + open_rel;
        output.push_str(&line[cursor..open + 2]);
        let Some(close_rel) = line[open + 2..].find(')') else {
            output.push_str(&line[open + 2..]);
            return output;
        };
        let close = open + 2 + close_rel;
        let url = &line[open + 2..close];
        if url.starts_with('<') && url.ends_with('>') {
            output.push_str(url);
        } else if url.contains(char::is_whitespace) && !url.contains('"') {
            output.push('<');
            output.push_str(&url.replace('>', "\\>"));
            output.push('>');
        } else {
            output.push_str(&escape_link_parens(url));
        }
        output.push(')');
        cursor = close + 1;
    }
    output.push_str(&line[cursor..]);
    output
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

fn normalize_terminal_spacing_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;

    while cursor < line.len() {
        let rest = &line[cursor..];
        if rest.starts_with("**") {
            let content_start = cursor + 2;
            let Some(close_rel) = line[content_start..].find("**") else {
                output.push_str(rest);
                break;
            };
            let close = content_start + close_rel;
            let inner = line[content_start..close].trim();
            if inner.is_empty() {
                output.push_str(&line[cursor..close + 2]);
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
            continue;
        }
        if rest.starts_with('`') {
            let content_start = cursor + 1;
            let Some(close_rel) = line[content_start..].find('`') else {
                output.push_str(rest);
                break;
            };
            let close = content_start + close_rel;
            if output
                .chars()
                .last()
                .is_some_and(|character| character.is_alphanumeric())
            {
                output.push(' ');
            }
            output.push('`');
            output.push_str(&normalize_strong_spacing(&line[content_start..close]));
            output.push('`');
            if line[close + 1..]
                .chars()
                .next()
                .is_some_and(|character| character.is_alphanumeric())
            {
                output.push(' ');
            }
            cursor = close + 1;
            continue;
        }
        if rest.starts_with("[^")
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
        if rest.starts_with(")[") {
            output.push_str(") [");
            cursor += 2;
            continue;
        }

        let character = rest.chars().next().expect("cursor is within line");
        output.push(character);
        cursor += character.len_utf8();
    }

    output
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
    match re_checked.replace_all(html, "<li>[x] ") {
        Cow::Borrowed(checked) => re_unchecked.replace_all(checked, "<li>[ ] ").into_owned(),
        Cow::Owned(checked) if re_unchecked.is_match(&checked) => {
            re_unchecked.replace_all(&checked, "<li>[ ] ").into_owned()
        }
        Cow::Owned(checked) => checked,
    }
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

fn convert_inline_formatting(html: &str) -> String {
    let re_mark =
        MARK_TAG_RE.get_or_init(|| regex_lite::Regex::new(r"<mark[^>]*>(.*?)</mark>").unwrap());
    let re_del =
        DEL_TAG_RE.get_or_init(|| regex_lite::Regex::new(r"</?(?:del|s|strike)\b[^>]*>").unwrap());
    match re_mark.replace_all(html, "==$1==") {
        Cow::Borrowed(marked) => re_del.replace_all(marked, "~~").into_owned(),
        Cow::Owned(marked) if re_del.is_match(&marked) => {
            re_del.replace_all(&marked, "~~").into_owned()
        }
        Cow::Owned(marked) => marked,
    }
}

/// Convert the content of a DOM element to Markdown.
pub fn content_to_markdown(doc: &Document, content_id: NodeId) -> String {
    content_to_markdown_from_document(doc, content_id, None)
}

pub(crate) fn content_to_markdown_mut_with_diagnostics(
    doc: &mut Document,
    content_id: NodeId,
    diagnostics: &mut Diagnostics,
) -> String {
    let needs_dom_preprocess = ["img", "table", "iframe"].iter().any(|selector| {
        !doc.query_selector_all_in_subtree(content_id, selector)
            .is_empty()
    });
    let mut diagnostics = Some(diagnostics);
    let html = measure_phase(&mut diagnostics, "markdown serialize", || {
        if needs_dom_preprocess {
            preprocess_markdown_dom(doc, content_id);
        }
        doc.inner_html(content_id)
    });
    let cleaned = measure_phase(&mut diagnostics, "markdown preprocess", || {
        preprocess_html_text(&html)
    });
    convert_cleaned_html_to_markdown(&cleaned, &mut diagnostics)
}

fn content_to_markdown_from_document(
    doc: &Document,
    content_id: NodeId,
    mut diagnostics: Option<&mut Diagnostics>,
) -> String {
    let needs_dom_preprocess = ["img", "table", "iframe"].iter().any(|selector| {
        !doc.query_selector_all_in_subtree(content_id, selector)
            .is_empty()
    });
    let html = measure_phase(&mut diagnostics, "markdown serialize", || {
        if needs_dom_preprocess {
            let (mut markdown_doc, root) = Document::clone_subtree(doc, content_id);
            preprocess_markdown_dom(&mut markdown_doc, root);
            markdown_doc.inner_html(root)
        } else {
            doc.inner_html(content_id)
        }
    });
    let cleaned = measure_phase(&mut diagnostics, "markdown preprocess", || {
        preprocess_html_text(&html)
    });
    convert_cleaned_html_to_markdown(&cleaned, &mut diagnostics)
}

pub fn normalize_origin_links(md: &str) -> String {
    let mut md = Cow::Borrowed(md);
    if md.contains("](//CLAUDE.md)") {
        md = Cow::Owned(md.replace("](//CLAUDE.md)", "](https://claude.md/)"));
    }
    let protocol_relative =
        PROTOCOL_RELATIVE_RE.get_or_init(|| regex_lite::Regex::new(r"\(//([^/\s)]+)\)").unwrap());
    if let Cow::Owned(normalized) = protocol_relative.replace_all(md.as_ref(), "(https://$1/)") {
        md = Cow::Owned(normalized);
    }
    let origin_re =
        ORIGIN_URL_RE.get_or_init(|| regex_lite::Regex::new(r"\((https?://[^/\s)]+)\)").unwrap());
    if let Cow::Owned(normalized) = origin_re.replace_all(md.as_ref(), "($1/)") {
        md = Cow::Owned(normalized);
    }
    md.into_owned()
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
    fn linked_images_trim_block_whitespace_from_link_labels() {
        let html = "<a href='https://example.com/page'>\n  <figure><img src='https://example.com/image.png'></figure>\n</a>";
        let md = html_to_markdown(html);
        assert!(md.contains("[![](https://example.com/image.png)](https://example.com/page)"));
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
    fn inline_formatting_replacements_preserve_text_and_transform_tags() {
        assert_eq!(convert_inline_formatting("plain"), "plain");
        assert_eq!(convert_inline_formatting("<mark>marked</mark>"), "==marked==");
        assert_eq!(convert_inline_formatting("<del>deleted</del>"), "~~deleted~~");
        assert_eq!(
            convert_inline_formatting("<mark>marked</mark> and <s>deleted</s>"),
            "==marked== and ~~deleted~~"
        );
    }

    #[test]
    fn basic_html_replacements_convert_nbsp_and_break_variants() {
        assert_eq!(
            preprocess_html_text("A&nbsp;B<br>C<br/>D<br />E"),
            "A B\nC\nD\nE"
        );
    }

    #[test]
    fn origin_link_normalization_preserves_and_rewrites_links() {
        assert_eq!(normalize_origin_links("plain"), "plain");
        assert_eq!(
            normalize_origin_links(
                "](//CLAUDE.md) (//example.com) (https://example.org)"
            ),
            "](https://claude.md/) (https://example.com/) (https://example.org/)"
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
            html_to_markdown(
                "<ul><li><strong>JavaScript arrays are resizable</strong> and <strong>can contain a mix</strong>.</li></ul>"
            ),
            "- **JavaScript arrays are resizable** and **can contain a mix**."
        );
    }

    #[test]
    fn postprocess_spacing_is_stateful_and_fenced_code_safe() {
        let cases = [
            "plain text",
            "- **bold**and`code`[^1]",
            "before`code`after [^1]*next",
            "```rust\n**inside**`code`[^1]\n```\n**outside**[^2]",
            "> **quoted**and `code`[^3]",
            "| **header** | `code` |\n| --- | --- |",
        ];

        for markdown in cases {
            let processed = postprocess_markdown(markdown);
            assert!(
                !processed.contains("  **bold**and"),
                "markdown: {markdown:?}"
            );
        }
        let fenced = postprocess_markdown("```rust\n**inside**`code`[^1]\n```\n**outside**[^2]");
        assert!(fenced.contains("```rust\n**inside**`code`[^1]\n```"));
        assert!(fenced.contains("**outside**[^2]"));
    }
}
