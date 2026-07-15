use crate::dom::{Document, NodeId};
use htmd::element_handler::{HandlerResult, Handlers};
use htmd::{
    Element, HtmlToMarkdown,
    options::{HeadingStyle, LinkStyle},
};

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

    // --- highlight: <mark> → ==text== ---
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

    // --- strikethrough: <del>/<s>/<strike> → ~~text~~ ---
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

    // --- embedToMarkdown: iframe YouTube/Twitter → links ---
    builder = builder.add_handler(
        vec!["iframe"],
        |_h: &dyn Handlers, el: Element| -> Option<HandlerResult> {
            let src = el
                .attrs
                .iter()
                .find(|a| a.name.local.as_ref() == "src")
                .map(|a| a.value.as_ref())
                .unwrap_or("");
            if src.contains("youtube.com/embed/") || src.contains("youtube-nocookie.com/embed/") {
                let video_id = src
                    .split("/embed/")
                    .nth(1)
                    .and_then(|s| s.split(['?', '&', '"']).next())
                    .unwrap_or("");
                Some(HandlerResult {
                    content: format!("[](https://www.youtube.com/watch?v={video_id})"),
                    markdown_translated: true,
                })
            } else if src.contains("platform.twitter.com/embed") {
                let tweet_id = src
                    .split("id=")
                    .nth(1)
                    .and_then(|s| s.split(['&', '"']).next())
                    .unwrap_or("");
                Some(HandlerResult {
                    content: format!("[](https://x.com/i/status/{tweet_id})"),
                    markdown_translated: true,
                })
            } else {
                None // Let default handler deal with it
            }
        },
    );

    // --- citations: <sup id="fnref:N"> → [^N] ---
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
                        let href = attrs
                            .borrow()
                            .iter()
                            .find(|attr| attr.name.local.as_ref() == "href")
                            .map(|attr| attr.value.to_string())
                            .unwrap_or_default();
                        output.push_str(&format!("[{}]({})", inner.trim(), href));
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

    // --- footnotes: <div id="footnotes"> <ol> <li id="fn:N"> → [^N]: text ---
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
                                let url = String::new();
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
                                                if href.starts_with("#fnref:") {
                                                    // Backref link — skip
                                                    continue;
                                                }
                                                if !text.is_empty() && !text.ends_with(' ') {
                                                    text.push(' ');
                                                }
                                                let mut link_text = String::new();
                                                collect_rcdom_text(lc, &mut link_text);
                                                text.push_str(&format!(
                                                    "[{}]({})",
                                                    link_text.trim(),
                                                    href
                                                ));
                                                text.push(' ');
                                            } else {
                                                // Collect text from other elements recursively
                                                collect_rcdom_markdown(lc, &mut text);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                let trimmed = text.trim();
                                if !trimmed.is_empty() {
                                    if !url.is_empty() {
                                        result
                                            .push_str(&format!("[^{num}]: [{trimmed}]({url})\n\n"));
                                    } else {
                                        result.push_str(&format!("[^{num}]: {trimmed}\n\n"));
                                    }
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

    // --- callout: div[data-callout] → blockquote (ported from upstream callout rule) ---
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
    let re_wbr = regex_lite::Regex::new(r"<wbr\s*/?>").unwrap();
    let html = re_wbr.replace_all(&html, "").to_string();

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

    // Handle code block language detection from data-lang or class language-*
    let html = convert_code_block_language(&html);

    // Handle YouTube/Twitter embed iframes → links
    let html = convert_embeds_to_links(&html);

    // Extract MathML data-latex attributes for LaTeX rendering
    let html = extract_math_latex(&html);

    // Strip non-essential table attributes
    let html = strip_table_attrs(&html);

    // Resolve best image sources from srcset

    resolve_best_image_sources(&html)
}

/// Postprocess markdown to clean up artifacts.
/// Ported from TypeScript createMarkdownContent post-processing.
fn postprocess_markdown(md: &str) -> String {
    let md = escape_tag_like_text(md);

    // Remove the page title h1 from the beginning if present.
    // Note: h1 was promoted to h2 by standardize_headings, so only
    // original h2 headers survive; we match ^# (h1) only like upstream.
    let title_re = regex_lite::Regex::new(r"^# [^\n]+\n+").unwrap();
    let md = title_re.replace(&md, "").to_string();

    // Remove any empty links [](url) that remain, along with surrounding newlines
    // But don't affect image links like ![](image.jpg)
    // Strategy: find all [](url) that are NOT preceded by !
    let empty_link_re = regex_lite::Regex::new(r"\[\]\([^)]+\)").unwrap();
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

    // Normalize horizontal rules: htmd produces * * *, upstream expects ---
    let md = md.replace("\n\n* * *\n\n", "\n\n---\n\n");

    let md = fix_inline_spacing_outside_code(&md);

    // htmd escapes the bracket syntax produced for extractor footnote references.
    let footnote_escape_re = regex_lite::Regex::new(r#"\\(\[\^\d+)\\\]"#).unwrap();
    let md = footnote_escape_re.replace_all(&md, "$1]").to_string();

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

        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_blank {
                result.push('\n');
                prev_blank = true;
            }
        } else {
            result.push_str(trimmed);
            result.push('\n');
            prev_blank = false;
        }
    }

    // Replace dot-spacer (produced by extractors for title separation)
    // with a single-space paragraph. Must run after line-by-line cleanup
    // which would otherwise trim the space line.
    let result = result.replace("\n\n\u{00B7}\n\n", "\n\n \n\n");
    let result = normalize_markdown_layout(&result);
    let result = regex_lite::Regex::new(r"\*([^*\n]+) \*")
        .unwrap()
        .replace_all(&result, "*$1*")
        .to_string();

    result.trim_end().to_string()
}

fn normalize_markdown_layout(md: &str) -> String {
    let mut out = String::new();
    let mut in_fenced_code = false;

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
            line = format!("| {} |", cells.join(" | "));
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
    let checklist_re = regex_lite::Regex::new(r"(?m)^(✅ .+)\n\n(✅ )").unwrap();
    checklist_re.replace_all(&result, "$1  \n$2").to_string()
}

/// Escape tag-like sequences in text (e.g. "Monte<video>: A Real Place")
/// so CommonMark renderers don't parse them as raw HTML and swallow
/// following content. Ported from upstream markdown.ts turndownService.escape.
fn escape_tag_like_text(md: &str) -> String {
    let tag_like_re = regex_lite::Regex::new(r"</?[A-Za-z][A-Za-z0-9-]*(?:\s[^>]*)?/?>").unwrap();
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
            result.push_str(
                &tag_like_re.replace_all(line, |caps: &regex_lite::Captures| {
                    format!("\\{}", &caps[0])
                }),
            );
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
    let multi_nl_re = regex_lite::Regex::new(r"\n{3,}").unwrap();

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
    let line = regex_lite::Regex::new(r"`\s*([^`]*?)\s*`")
        .unwrap()
        .replace_all(line, |c: &regex_lite::Captures| format!("`{}`", &c[1]))
        .to_string();
    let line = regex_lite::Regex::new(r"([a-zA-Z0-9])(\*\*[^*]+\*\*)")
        .unwrap()
        .replace_all(&line, |c: &regex_lite::Captures| {
            format!("{} {}", &c[1], &c[2])
        })
        .to_string();
    let line = regex_lite::Regex::new(r"(\*\*[^*]+\*\*)([a-zA-Z0-9])")
        .unwrap()
        .replace_all(&line, |c: &regex_lite::Captures| {
            format!("{} {}", &c[1], &c[2])
        })
        .to_string();
    let line = regex_lite::Regex::new(r"([a-zA-Z0-9])(\*[^*]+\*)")
        .unwrap()
        .replace_all(&line, |c: &regex_lite::Captures| {
            format!("{} {}", &c[1], &c[2])
        })
        .to_string();
    let line = regex_lite::Regex::new(r"(\*[^*]+\*)([a-zA-Z0-9])")
        .unwrap()
        .replace_all(&line, |c: &regex_lite::Captures| {
            format!("{} {}", &c[1], &c[2])
        })
        .to_string();
    let line = regex_lite::Regex::new(r"\*([^*\n]+) \*")
        .unwrap()
        .replace_all(&line, "*$1*")
        .to_string();
    let line = regex_lite::Regex::new(r"\* +([^*\n]+?) +\*")
        .unwrap()
        .replace_all(&line, "*$1*")
        .to_string();
    let line = regex_lite::Regex::new(r"\* ([^*\n]+)\*")
        .unwrap()
        .replace_all(&line, "*$1*")
        .to_string();
    let line = regex_lite::Regex::new(r" (\* )([A-Za-z])")
        .unwrap()
        .replace_all(&line, " *$2")
        .to_string();
    let line = regex_lite::Regex::new(r"(\*[^*\n]+\*) ([.,;:!?])")
        .unwrap()
        .replace_all(&line, "$1$2")
        .to_string();
    let line = regex_lite::Regex::new(r"([A-Za-z0-9,.;:!?])(\*[^*\n]+\*)")
        .unwrap()
        .replace_all(&line, "$1 $2")
        .to_string();
    let line = regex_lite::Regex::new(r"\*([^*\n]+)\*([A-Za-z0-9(])")
        .unwrap()
        .replace_all(&line, "*$1* $2")
        .to_string();
    let line = regex_lite::Regex::new(r"(\[\^\d+\]) ([.,;:!?])")
        .unwrap()
        .replace_all(&line, "$1$2")
        .to_string();
    let line = regex_lite::Regex::new(r"\\(-)( )")
        .unwrap()
        .replace_all(&line, "$1$2")
        .to_string();
    let line = regex_lite::Regex::new(r"([a-zA-Z0-9:;,.!?])(`[^`]+`)")
        .unwrap()
        .replace_all(&line, |c: &regex_lite::Captures| {
            format!("{} {}", &c[1], &c[2])
        })
        .to_string();
    let line = regex_lite::Regex::new(r"(`[^`]+`)([a-zA-Z0-9])")
        .unwrap()
        .replace_all(&line, |c: &regex_lite::Captures| {
            format!("{} {}", &c[1], &c[2])
        })
        .to_string();
    let line = regex_lite::Regex::new(r"([a-zA-Z0-9])(\[[^\]]+\]\([^)]+\))")
        .unwrap()
        .replace_all(&line, |c: &regex_lite::Captures| {
            format!("{} {}", &c[1], &c[2])
        })
        .to_string();
    let line = regex_lite::Regex::new(r"(\[[^\]]+\]\([^)]+\))([a-zA-Z0-9])")
        .unwrap()
        .replace_all(&line, |c: &regex_lite::Captures| {
            format!("{} {}", &c[1], &c[2])
        })
        .to_string();
    let line = regex_lite::Regex::new(r"(\[[^\]]+\]\([^)]+\))(- )")
        .unwrap()
        .replace_all(&line, "$1 $2")
        .to_string();
    let line = regex_lite::Regex::new(r"(\[[^\]]+\]\([^)]+\)) {2,}")
        .unwrap()
        .replace_all(&line, "$1 ")
        .to_string();
    let line = regex_lite::Regex::new(r#"([A-Za-z0-9"])(\[\^\d+\])"#)
        .unwrap()
        .replace_all(&line, "$1 $2")
        .to_string();
    let line = regex_lite::Regex::new(r"(`[^`]+`)(\[\^\d+\])")
        .unwrap()
        .replace_all(&line, "$1 $2")
        .to_string();
    let line = regex_lite::Regex::new(r"(\S) \* ([A-Z])")
        .unwrap()
        .replace_all(&line, "$1 *$2")
        .to_string();
    let line = regex_lite::Regex::new(r"([A-Za-z])(\*\*\*)")
        .unwrap()
        .replace_all(&line, "$1 $2")
        .to_string();
    let line = regex_lite::Regex::new(r"(,)(\*\[)")
        .unwrap()
        .replace_all(&line, "$1 $2")
        .to_string();
    let line = regex_lite::Regex::new(r"` +([^`\n]*?)`")
        .unwrap()
        .replace_all(&line, "`$1`")
        .to_string();
    let line = regex_lite::Regex::new(r"`([^`\n]*?) +`")
        .unwrap()
        .replace_all(&line, "`$1`")
        .to_string();
    let line = regex_lite::Regex::new(r"`([^`\n]*)`([A-Za-z])")
        .unwrap()
        .replace_all(&line, "`$1` $2")
        .to_string();
    let line = regex_lite::Regex::new(r"` +([^`\n]*)`")
        .unwrap()
        .replace_all(&line, "`$1`")
        .to_string();
    regex_lite::Regex::new(r"\\\\<([A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,})>")
        .unwrap()
        .replace_all(&line, |c: &regex_lite::Captures| format!("\\<{}>", &c[1]))
        .to_string()
}

/// Format markdown link destinations: wrap URLs with spaces in <>, escape parens.
fn format_link_destinations(md: &str) -> String {
    let link_re = regex_lite::Regex::new(r"\]\(([^)]+)\)").unwrap();
    link_re
        .replace_all(md, |caps: &regex_lite::Captures| {
            let url = caps.get(1).unwrap().as_str();
            let formatted = if url.contains(char::is_whitespace) {
                format!("<{}>", url.replace('>', "\\>"))
            } else {
                url.replace('(', "\\(").replace(')', "\\)")
            };
            format!("]({})", formatted)
        })
        .to_string()
}

/// Convert Obsidian-style callout blocks to blockquote format.
#[allow(dead_code)]
fn convert_callouts_to_markdown(html: &str) -> String {
    // Match div with data-callout attribute (order-independent)
    let re =
        regex_lite::Regex::new(r##"<div[^>]*\bdata-callout="([^"]*)"[^>]*>(.*?)</div>"##).unwrap();
    re.replace_all(html, |caps: &regex_lite::Captures| {
        let ct = caps.get(1).map_or("", |m| m.as_str());
        let content = caps.get(2).map_or("", |m| m.as_str());

        // Extract title from callout-title-inner if present
        let title_re = regex_lite::Regex::new(
            r#"<div[^>]*class="[^"]*\bcallout-title-inner\b[^"]*"[^>]*>(.*?)</div>"#,
        )
        .unwrap();
        let title = if let Some(tc) = title_re.captures(content) {
            tc.get(1).map_or("", |m| m.as_str())
        } else {
            ""
        };

        // Extract body from callout-content if present
        let body_re = regex_lite::Regex::new(
            r#"<div[^>]*class="[^"]*\bcallout-content\b[^"]*"[^>]*>(.*?)</div>"#,
        )
        .unwrap();
        let body = if let Some(bc) = body_re.captures(content) {
            bc.get(1).map_or("", |m| m.as_str())
        } else {
            content
        };
        // Clean body: strip remaining HTML tags except <strong>, <em>, <code>, <a>
        let body = strip_keep_inline_tags(body);

        let fold_marker = if ct.starts_with('-') { "- " } else { "" };
        let clean_ct = ct.trim_start_matches('-');
        let type_upper: String = clean_ct
            .chars()
            .enumerate()
            .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
            .collect();
        let title_part = if title.is_empty() {
            format!("[!{}]", type_upper)
        } else {
            format!("[!{}] {}", type_upper, title)
        };
        let mut out = format!("> {}{}\n>\n> {}", fold_marker, title_part, body);
        // Replace newlines in body with \n>
        out = out.replace("\n", "\n> ");
        format!("\n{}\n", out)
    })
    .to_string()
}

#[allow(dead_code)]
fn strip_keep_inline_tags(s: &str) -> String {
    // Keep: strong, em, b, i, code, a, p, br, mark, del, s, strike, sup, sub, span
    let keep_tags = [
        "strong", "em", "b", "i", "code", "a", "p", "br", "mark", "del", "s", "strike", "sup",
        "sub", "span",
    ];
    let re = regex_lite::Regex::new(r"</?(\w+)(?:\s[^>]*)?>").unwrap();
    re.replace_all(s, |caps: &regex_lite::Captures| {
        let tag = caps.get(1).map_or("", |m| m.as_str()).to_lowercase();
        if keep_tags.contains(&tag.as_str()) {
            caps.get(0).unwrap().as_str().to_string()
        } else {
            String::new()
        }
    })
    .to_string()
}

/// Convert footnote references and lists.
fn convert_footnotes_to_markdown(html: &str) -> String {
    let re = regex_lite::Regex::new(
        r##"<sup[^>]*class="[^"]*\bfootnote-ref\b[^"]*"[^>]*>\s*<a[^>]*href="#fn(\d+)"[^>]*>.*?</a>\s*</sup>"##
    ).unwrap();
    re.replace_all(html, "[^$1]").to_string()
}

/// Convert task list checkboxes to markdown format.
fn convert_task_lists(html: &str) -> String {
    let re_checked =
        regex_lite::Regex::new(r#"<li[^>]*>\s*<input[^>]*type="checkbox"[^>]*checked[^>]*>\s*"#)
            .unwrap();
    let re_unchecked =
        regex_lite::Regex::new(r#"<li[^>]*>\s*<input[^>]*type="checkbox"[^>]*>\s*"#).unwrap();
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
    let re_img = regex_lite::Regex::new(r#"<img([^>]*)srcset="([^"]*)"([^>]*)>"#).unwrap();
    re_img
        .replace_all(html, |caps: &regex_lite::Captures| {
            let before = caps.get(1).map_or("", |m| m.as_str());
            let srcset = caps.get(2).map_or("", |m| m.as_str());
            let after = caps.get(3).map_or("", |m| m.as_str());

            if let Some(best_url) = get_best_image_src(srcset) {
                // Check if src is already set
                if before.contains("src=") || after.contains("src=") {
                    // Keep existing src, but use best from srcset as data attribute
                    format!(
                        r#"<img{}src="{}" data-srcset="{}"{}>"#,
                        before, best_url, srcset, after
                    )
                } else {
                    format!(r#"<img{}src="{}"{}>"#, before, best_url, after)
                }
            } else {
                caps.get(0).unwrap().as_str().to_string()
            }
        })
        .to_string()
}

/// Convert inline formatting: <mark> → ==text==, <del>/<s>/<strike> → ~~text~~
fn convert_inline_formatting(html: &str) -> String {
    let re_mark = regex_lite::Regex::new(r"<mark[^>]*>(.*?)</mark>").unwrap();
    let re_del = regex_lite::Regex::new(r"</?(?:del|s|strike)\b[^>]*>").unwrap();
    let html = re_mark.replace_all(html, "==$1==").to_string();
    re_del.replace_all(&html, "~~").to_string()
}

/// Detect code block language from data-lang or class language-* attributes.
fn convert_code_block_language(html: &str) -> String {
    let re = regex_lite::Regex::new(
        r#"<pre[^>]*>(?:\s*<code[^>]*(?:data-lang="([^"]*)"[^>]*|class="[^"]*language-([^" ]*)[^"]*"[^>]*)[^>]*>)"#
    ).unwrap();
    re.replace_all(html, |caps: &regex_lite::Captures| {
        let data_lang = caps.get(1).map_or("", |m| m.as_str());
        let class_lang = caps.get(2).map_or("", |m| m.as_str());
        let _lang = if !data_lang.is_empty() {
            data_lang
        } else {
            class_lang
        };
        caps.get(0).unwrap().as_str().to_string()
    })
    .to_string()
}

fn convert_embeds_to_links(html: &str) -> String {
    let re_yt = regex_lite::Regex::new(
        r#"<iframe[^>]*src="https?://(?:www\.)?youtube\.com/embed/([^"?]+)[^"]*"[^>]*></iframe>"#,
    )
    .unwrap();
    let re_tw = regex_lite::Regex::new(
        r#"<iframe[^>]*src="https?://platform\.twitter\.com/embed/[^"]*"[^>]*></iframe>"#,
    )
    .unwrap();
    let html = re_yt
        .replace_all(html, "[YouTube](https://www.youtube.com/watch?v=$1)")
        .to_string();
    re_tw.replace_all(&html, "[Twitter/X Embed]").to_string()
}

/// Extract data-latex attribute from <math> elements and render as LaTeX block.
fn extract_math_latex(html: &str) -> String {
    let re = regex_lite::Regex::new(r#"<math[^>]*data-latex="([^"]*)"[^>]*>.*?</math>"#).unwrap();
    re.replace_all(html, |caps: &regex_lite::Captures| {
        let latex = caps.get(1).unwrap().as_str();
        if latex.contains("\\begin{") {
            format!("\n$\n{}\n$\n", latex)
        } else {
            format!("${}$", latex)
        }
    })
    .to_string()
}

/// Strip non-essential attributes from table HTML.
fn strip_table_attrs(html: &str) -> String {
    let re = regex_lite::Regex::new(r#"<table[^>]*>"#).unwrap();
    re.replace_all(html, "<table>").to_string()
}

/// Convert the content of a DOM element to Markdown.
pub fn content_to_markdown(doc: &Document, content_id: NodeId) -> String {
    let html = doc.inner_html(content_id);
    html_to_markdown(&html)
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
}
