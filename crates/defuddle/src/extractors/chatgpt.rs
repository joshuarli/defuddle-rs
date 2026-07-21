use crate::dom::Document;
use crate::extractors::{HtmlExtraction, SiteAdapter, SiteExtraction};
use crate::pipeline::PipelineContext;
use crate::utils::escape_html;
use rustc_hash::FxHashMap;

pub(crate) fn adapter() -> Box<dyn SiteAdapter> {
    Box::new(ChatGptAdapter)
}

struct ChatGptAdapter;

impl SiteAdapter for ChatGptAdapter {
    fn matches(&self, context: &PipelineContext<'_>) -> bool {
        !context
            .doc
            .query_selector_all("[data-testid^=\"conversation-turn-\"]")
            .is_empty()
    }

    fn extract(&self, context: &mut PipelineContext<'_>) -> SiteExtraction {
        let result = ChatGPTExtractor.extract(&context.doc);
        crate::extractors::site_extraction_from_html(result.content_html, result.variables)
    }
}

struct ChatGPTExtractor;

impl ChatGPTExtractor {
    fn extract(&self, doc: &Document) -> HtmlExtraction {
        let title = doc
            .query_selector_all("title")
            .first()
            .map(|id| doc.text_content(*id).trim().to_string())
            .unwrap_or_else(|| "ChatGPT Conversation".into());

        let turns = doc.query_selector_all("[data-testid^=\"conversation-turn-\"]");
        let mut messages: Vec<(String, String)> = Vec::new();
        let mut footnotes: Vec<(String, String)> = Vec::new();
        let mut footnote_counter: usize = 0;

        for turn_id in turns {
            let heading = doc
                .query_selector_all_in_subtree(turn_id, "h4.sr-only, h5.sr-only, h6.sr-only")
                .first()
                .map(|&id| {
                    doc.text_content(id)
                        .trim()
                        .trim_end_matches(':')
                        .to_string()
                })
                .unwrap_or_default();

            // Collect all content from [data-message-author-role] elements (post-thought merge)
            let msg_els: Vec<_> = doc
                .query_selector_all_in_subtree(
                    turn_id,
                    "[data-message-author-role=\"user\"],[data-message-author-role=\"assistant\"]",
                )
                .into_iter()
                .filter(|&el| {
                    doc.closest(el, "[data-testid^=\"conversation-turn-\"]") == Some(turn_id)
                })
                .collect();

            let content_parts: Vec<String> = if msg_els.is_empty() {
                let fallback = doc
                    .query_selector_all_in_subtree(turn_id, ".markdown, .whitespace-pre-wrap")
                    .first()
                    .map(|&id| doc.inner_html(id))
                    .unwrap_or_else(|| doc.inner_html(turn_id));
                vec![fallback]
            } else {
                msg_els
                    .iter()
                    .flat_map(|&el| {
                        let content_els = doc
                            .query_selector_all_in_subtree(el, ".markdown, .whitespace-pre-wrap");
                        if content_els.is_empty() {
                            vec![doc.inner_html(el)]
                        } else {
                            content_els.iter().map(|&c| doc.inner_html(c)).collect()
                        }
                    })
                    .collect()
            };

            let mut message_content = content_parts.join("\n");
            message_content = message_content.replace('\u{200B}', "");
            message_content = message_content.replace("<br>", "\n");

            // Process citations
            message_content =
                process_citations(&message_content, &mut footnotes, &mut footnote_counter);

            // Clean up empty paragraphs
            use std::sync::LazyLock;

            static EMPTY_P: LazyLock<regex_lite::Regex> =
                LazyLock::new(|| regex_lite::Regex::new(r"<p[^>]*>\s*</p>").unwrap());
            message_content = EMPTY_P.replace_all(&message_content, "").to_string();

            let trimmed = message_content.trim().to_string();
            if !trimmed.is_empty() {
                messages.push((heading, trimmed));
            }
        }

        let html = build_conversation_html(&messages, &footnotes);

        HtmlExtraction {
            content_html: html,
            variables: FxHashMap::from_iter([
                ("title".into(), title),
                ("site".into(), "ChatGPT".into()),
                ("author".into(), String::new()),
                ("published".into(), String::new()),
            ]),
        }
    }
}

fn process_citations(
    html: &str,
    footnotes: &mut Vec<(String, String)>,
    counter: &mut usize,
) -> String {
    use std::sync::LazyLock;

    static CITATION: LazyLock<regex_lite::Regex> = LazyLock::new(|| {
        regex_lite::Regex::new(
            r#"<span[^>]*data-testid="webpage-citation-pill"[^>]*>[\s\S]*?<a\b[^>]*href="([^"]*)"[^>]*>[\s\S]*?</a>[\s\S]*?</span>"#,
        )
        .unwrap()
    });
    CITATION.replace_all(html, |caps: &regex_lite::Captures| {
        let url = caps
            .get(1)
            .map(|m| m.as_str().replace("&amp;", "&"))
            .unwrap_or_default();
        let domain = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url.as_str())
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .trim_start_matches("www.");
        if domain.is_empty() {
            return String::new();
        }
        let existing = footnotes.iter().position(|(u, _)| u == &url);
        let num = if let Some(idx) = existing {
            idx + 1
        } else {
            *counter += 1;
            footnotes.push((url.clone(), domain.to_string()));
            *counter
        };
        format!("<sup class=\"footnote-ref\"><a href=\"#fn{num}\" id=\"fnref:{num}\">{num}</a></sup>")
    })
    .to_string()
}

fn build_conversation_html(
    messages: &[(String, String)],
    footnotes: &[(String, String)],
) -> String {
    let mut html = String::new();
    for (i, (author, msg)) in messages.iter().enumerate() {
        if i > 0 {
            html.push_str("<hr>\n");
        }
        let safe = escape_html(author);
        html.push_str(&format!(
            "<div class=\"message message-{}\">\n<div class=\"message-header\">\n<p class=\"message-author\"><strong>{}</strong></p>\n</div>\n<div class=\"message-content\">\n{}\n</div>\n</div>\n",
            author.to_lowercase(), safe, msg
        ));
    }
    if !footnotes.is_empty() {
        html.push_str("<div id=\"footnotes\">\n<ol>\n");
        for (i, (url, domain)) in footnotes.iter().enumerate() {
            let n = i + 1;
            html.push_str(&format!(
                "<li class=\"footnote\" id=\"fn:{n}\">\n<p>\n<a href=\"{url}\" target=\"_blank\">{domain}</a>&nbsp;<a href=\"#fnref:{n}\" class=\"footnote-backref\">↩</a>\n</p>\n</li>\n"
            ));
        }
        html.push_str("</ol>\n</div>\n");
    }
    html.trim().to_string()
}
