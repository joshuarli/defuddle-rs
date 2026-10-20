use crate::dom::{Document, NodeId};
use crate::extractors::{HtmlExtraction, SiteAdapter, SiteExtraction, comments};
use crate::pipeline::ExtractionContext;
use rustc_hash::FxHashMap;

pub(crate) fn adapter() -> Box<dyn SiteAdapter> {
    Box::new(GmailAdapter)
}

struct GmailAdapter;

impl SiteAdapter for GmailAdapter {
    fn matches(&self, context: &ExtractionContext<'_>) -> bool {
        !context.doc.query_selector_all(".adn.ads").is_empty()
    }

    fn extract(&self, context: &mut ExtractionContext<'_>) -> SiteExtraction {
        let extraction = GmailExtractor.extract(&context.doc);
        crate::extractors::site_extraction_from_html(extraction.content_html, extraction.variables)
    }
}

struct GmailExtractor;

struct EmailMessage {
    author: String,
    email: String,
    date: String,
    content: String,
}

impl GmailExtractor {
    fn extract(&self, doc: &Document) -> HtmlExtraction {
        let messages = self.extract_messages(doc);
        let comment_data: Vec<_> = messages
            .iter()
            .map(|message| comments::CommentData {
                author: message.author.clone(),
                date: message.date.clone(),
                content: message.content.clone(),
                depth: 0,
                score: String::new(),
                url: String::new(),
            })
            .collect();
        let comments_html = comments::build_comment_tree(&comment_data);
        let content_html = format!(
            "<article data-defuddle><div class=\"gmail comments\">{comments_html}</div></article>"
        );

        let first = messages.first();
        let title = get_subject(doc);
        let mut variables = FxHashMap::from_iter([
            ("title".into(), title),
            (
                "author".into(),
                first
                    .map(|message| message.author.clone())
                    .unwrap_or_default(),
            ),
            ("site".into(), "Gmail".into()),
            (
                "published".into(),
                first
                    .map(|message| to_iso_date(&message.date))
                    .unwrap_or_default(),
            ),
        ]);
        variables.insert("messageCount".into(), messages.len().to_string());
        if let Some(email) = first
            .map(|message| message.email.as_str())
            .filter(|email| !email.is_empty())
        {
            variables.insert("postAuthor".into(), email.to_string());
        }

        HtmlExtraction {
            content_html,
            variables,
        }
    }

    fn extract_messages(&self, doc: &Document) -> Vec<EmailMessage> {
        let mut messages = Vec::new();
        for row in doc.query_selector_all(".adn.ads") {
            let sender = doc
                .query_selector_all_in_subtree(row, ".gD")
                .first()
                .copied();
            let author = sender
                .and_then(|id| doc.get_attr(id, "name"))
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .or_else(|| sender.map(|id| doc.text_content(id).trim().to_string()))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "Unknown".into());
            let email = sender
                .and_then(|id| doc.get_attr(id, "email"))
                .unwrap_or_default()
                .trim()
                .to_string();
            let date = doc
                .query_selector_all_in_subtree(row, ".g3")
                .first()
                .and_then(|&id| {
                    doc.get_attr(id, "title")
                        .or_else(|| doc.get_attr(id, "alt"))
                        .map(str::trim)
                        .filter(|date| !date.is_empty())
                        .map(str::to_string)
                })
                .or_else(|| {
                    doc.query_selector_all_in_subtree(row, ".g3")
                        .first()
                        .map(|&id| doc.text_content(id).trim().to_string())
                })
                .unwrap_or_default();
            let content = self.get_message_body(doc, row);
            if content.is_empty() {
                continue;
            }
            messages.push(EmailMessage {
                author,
                email,
                date,
                content,
            });
        }
        messages
    }

    fn get_message_body(&self, source: &Document, row: NodeId) -> String {
        // The message rows belong to the caller's document. Clone the document so
        // quote and Gmail-chrome removal cannot mutate the page used by metadata or
        // another adapter, while retaining stable NodeId indexes for this pass.
        let mut doc = source.clone();
        let Some(body) = doc
            .query_selector_all_in_subtree(row, ".a3s")
            .first()
            .copied()
        else {
            return String::new();
        };

        const REMOVE_SELECTORS: &str = concat!(
            ".gmail_quote, .gmail_attr, .gmail_extra, ",
            "blockquote[type=\"cite\"], .moz-cite-prefix, ",
            "blockquote[style*=\"border-left\"], .yahoo_quoted, ",
            ".a6S, .adL, .h5, .adm, .ajR, .ajT, .h4, .yj6qo"
        );
        for node in doc.query_selector_all_in_subtree(body, REMOVE_SELECTORS) {
            doc.remove(node);
        }
        strip_plain_text_quotes(&mut doc, body);
        trim_trailing_blanks(&mut doc, body);
        doc.inner_html(body).trim().to_string()
    }
}

fn get_subject(doc: &Document) -> String {
    if let Some(subject) = doc
        .query_selector_all("h2.hP")
        .first()
        .map(|&id| doc.text_content(id).trim().to_string())
        .filter(|subject| !subject.is_empty())
    {
        return subject;
    }

    let title = doc
        .query_selector_all("title")
        .first()
        .map(|&id| doc.text_content(id).trim().to_string())
        .unwrap_or_default();
    let Some(without_gmail) = title.strip_suffix(" - Gmail") else {
        return if title.is_empty() {
            "Gmail thread".into()
        } else {
            title
        };
    };
    without_gmail
        .rsplit_once(" - ")
        .map(|(subject, _)| subject.trim().to_string())
        .filter(|subject| !subject.is_empty())
        .unwrap_or_else(|| without_gmail.trim().to_string())
}

fn to_iso_date(date: &str) -> String {
    let tokens: Vec<_> = date.split_whitespace().collect();
    for (index, token) in tokens.iter().enumerate() {
        let Some(month) = month_number(token) else {
            continue;
        };
        let (day, year) = if index > 0 && index + 1 < tokens.len() {
            let before = parse_date_number(tokens[index - 1]);
            let after = parse_date_number(tokens[index + 1]);
            if before.is_some() && after.is_some() {
                (before, after)
            } else if index + 2 < tokens.len() {
                (
                    parse_date_number(tokens[index + 1]),
                    parse_date_number(tokens[index + 2]),
                )
            } else {
                (None, None)
            }
        } else if index + 2 < tokens.len() {
            (
                parse_date_number(tokens[index + 1]),
                parse_date_number(tokens[index + 2]),
            )
        } else {
            (None, None)
        };
        let (Some(day), Some(year)) = (day, year) else {
            continue;
        };
        if (1..=31).contains(&day) && (1000..=9999).contains(&year) {
            return format!("{year:04}-{month:02}-{day:02}");
        }
    }
    String::new()
}

fn parse_date_number(token: &str) -> Option<u32> {
    let digits = token.trim_matches(|character: char| !character.is_ascii_digit());
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

fn month_number(token: &str) -> Option<u32> {
    match token.trim_matches(|character: char| !character.is_ascii_alphabetic()) {
        month if month.eq_ignore_ascii_case("jan") || month.eq_ignore_ascii_case("january") => {
            Some(1)
        }
        month if month.eq_ignore_ascii_case("feb") || month.eq_ignore_ascii_case("february") => {
            Some(2)
        }
        month if month.eq_ignore_ascii_case("mar") || month.eq_ignore_ascii_case("march") => {
            Some(3)
        }
        month if month.eq_ignore_ascii_case("apr") || month.eq_ignore_ascii_case("april") => {
            Some(4)
        }
        month if month.eq_ignore_ascii_case("may") => Some(5),
        month if month.eq_ignore_ascii_case("jun") || month.eq_ignore_ascii_case("june") => Some(6),
        month if month.eq_ignore_ascii_case("jul") || month.eq_ignore_ascii_case("july") => Some(7),
        month if month.eq_ignore_ascii_case("aug") || month.eq_ignore_ascii_case("august") => {
            Some(8)
        }
        month
            if month.eq_ignore_ascii_case("sep")
                || month.eq_ignore_ascii_case("sept")
                || month.eq_ignore_ascii_case("september") =>
        {
            Some(9)
        }
        month if month.eq_ignore_ascii_case("oct") || month.eq_ignore_ascii_case("october") => {
            Some(10)
        }
        month if month.eq_ignore_ascii_case("nov") || month.eq_ignore_ascii_case("november") => {
            Some(11)
        }
        month if month.eq_ignore_ascii_case("dec") || month.eq_ignore_ascii_case("december") => {
            Some(12)
        }
        _ => None,
    }
}

fn collect_descendants(doc: &Document, node: NodeId, output: &mut Vec<NodeId>) {
    for child in doc.children(node).to_vec() {
        output.push(child);
        if doc.is_element(child) {
            collect_descendants(doc, child, output);
        }
    }
}

fn is_inline_tag(tag: &str) -> bool {
    matches!(
        tag,
        "a" | "abbr"
            | "b"
            | "bdi"
            | "bdo"
            | "big"
            | "button"
            | "cite"
            | "code"
            | "data"
            | "del"
            | "em"
            | "i"
            | "img"
            | "input"
            | "ins"
            | "kbd"
            | "label"
            | "mark"
            | "q"
            | "s"
            | "samp"
            | "small"
            | "span"
            | "strike"
            | "strong"
            | "sub"
            | "sup"
            | "time"
            | "tt"
            | "u"
            | "var"
            | "wbr"
    )
}

fn finish_line(lines: &mut Vec<Vec<NodeId>>, line: &mut Vec<NodeId>) {
    if !line.is_empty() {
        lines.push(std::mem::take(line));
    }
}

fn collect_lines(
    doc: &Document,
    parent: NodeId,
    lines: &mut Vec<Vec<NodeId>>,
    line: &mut Vec<NodeId>,
) {
    for child in doc.children(parent).to_vec() {
        if !doc.is_element(child) {
            line.push(child);
            continue;
        }
        let tag = doc.tag_name(child).to_ascii_lowercase();
        if tag == "br" {
            line.push(child);
            finish_line(lines, line);
        } else if is_inline_tag(&tag) {
            if doc.has_selector_in_subtree(child, "br") {
                collect_lines(doc, child, lines, line);
            } else {
                line.push(child);
            }
        } else {
            finish_line(lines, line);
            collect_lines(doc, child, lines, line);
            finish_line(lines, line);
        }
    }
}

fn strip_plain_text_quotes(doc: &mut Document, root: NodeId) {
    let mut descendants = Vec::new();
    collect_descendants(doc, root, &mut descendants);
    for node in descendants.iter().copied() {
        if doc.is_element(node) {
            continue;
        }
        let text = doc.text_content(node);
        if !text.contains('\n') {
            continue;
        }
        let mut filtered = String::with_capacity(text.len());
        for (index, line) in text.split('\n').enumerate() {
            if index > 0 {
                filtered.push('\n');
            }
            if !line.trim_start().starts_with('>') {
                filtered.push_str(line);
            }
        }
        if filtered.is_empty() {
            doc.remove(node);
        } else {
            doc.set_text(node, &filtered);
        }
    }

    let mut lines = Vec::new();
    let mut line = Vec::new();
    collect_lines(doc, root, &mut lines, &mut line);
    finish_line(&mut lines, &mut line);
    for nodes in lines {
        let text = nodes
            .iter()
            .map(|&node| doc.text_content(node))
            .collect::<String>();
        if text.trim_start().starts_with('>') {
            for node in nodes {
                doc.remove(node);
            }
        }
    }
}

fn has_media(doc: &Document, node: NodeId) -> bool {
    doc.tag_name_is_any(
        node,
        &[
            "img", "picture", "video", "audio", "iframe", "svg", "canvas", "object", "embed",
        ],
    ) || doc.has_selector_in_subtree(
        node,
        "img, picture, video, audio, iframe, svg, canvas, object, embed",
    )
}

fn is_blank(doc: &Document, node: NodeId) -> bool {
    if !doc.text_content(node).trim().is_empty() {
        return false;
    }
    !doc.is_element(node) || !has_media(doc, node)
}

fn trim_trailing_blanks(doc: &mut Document, root: NodeId) {
    while let Some(last) = doc.children(root).last().copied() {
        if !is_blank(doc, last) {
            break;
        }
        doc.remove(last);
    }
    if let Some(last) = doc
        .children(root)
        .last()
        .copied()
        .filter(|&node| doc.is_element(node))
    {
        trim_trailing_blanks(doc, last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_messages_and_removes_quoted_history() {
        let doc = Document::parse(
            r#"<html><head><title>Fallback - account@example.com - Gmail</title></head><body>
            <h2 class="hP">Project kickoff</h2>
            <div class="adn ads"><span class="gD" name="Alex Rivera" email="alex@example.com">Alex Rivera</span><span class="g3" title="May 13, 2026, 11:02 PM">date</span><div class="a3s"><div>Hello Jane.</div></div></div>
            <div class="adn ads"><span class="gD" name="Jane Doe" email="jane@example.com">Jane Doe</span><span class="g3" title="May 14, 2026, 9:15 AM">date</span><div class="a3s"><div>Thanks!</div><div class="gmail_quote"><blockquote>Quoted message</blockquote></div></div></div>
            </body></html>"#,
        );
        let extraction = GmailExtractor.extract(&doc);

        assert!(extraction.content_html.contains("Hello Jane."));
        assert!(extraction.content_html.contains("Thanks!"));
        assert!(!extraction.content_html.contains("Quoted message"));
        assert_eq!(extraction.variables["title"], "Project kickoff");
        assert_eq!(extraction.variables["author"], "Alex Rivera");
        assert_eq!(extraction.variables["published"], "2026-05-13");
        assert_eq!(extraction.variables["messageCount"], "2");
    }

    #[test]
    fn preserves_media_and_strips_plain_text_quote_lines() {
        let doc = Document::parse(
            r#"<div class="adn ads"><span class="gD" name="Alex">Alex</span><div class="a3s"><div>Current message<br><br>&gt; old message <a href="mailto:old@example.com">old@example.com</a><br><img src="https://example.com/photo.jpg"></div></div></div>"#,
        );
        let extraction = GmailExtractor.extract(&doc);

        assert!(extraction.content_html.contains("Current message"));
        assert!(!extraction.content_html.contains("old message"));
        assert!(!extraction.content_html.contains("old@example.com"));
        assert!(extraction.content_html.contains("photo.jpg"));
    }

    #[test]
    fn parses_english_calendar_dates_without_timezone_conversion() {
        assert_eq!(to_iso_date("Jan 5, 2026, 9:00 PM"), "2026-01-05");
        assert_eq!(to_iso_date("13 May 2026, 11:02 PM"), "2026-05-13");
        assert_eq!(to_iso_date("2026年5月13日 23:02"), "");
    }
}
