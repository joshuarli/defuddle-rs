use crate::content_utils::escape_html;
use crate::dom::Document;
use crate::extractors::{HtmlExtraction, SiteAdapter, SiteExtraction, comments};
use crate::pipeline::ExtractionContext;
use rustc_hash::FxHashMap;

pub(crate) fn adapter() -> Box<dyn SiteAdapter> {
    Box::new(TwitterAdapter)
}

struct TwitterAdapter;

impl SiteAdapter for TwitterAdapter {
    fn matches(&self, context: &ExtractionContext<'_>) -> bool {
        create(&context.doc, context.page_url).is_some()
    }

    fn extract(&self, context: &mut ExtractionContext<'_>) -> SiteExtraction {
        let twitter_extractor = create(&context.doc, context.page_url)
            .expect("Twitter adapter matched its document")
            .extract(&context.doc);
        crate::extractors::site_extraction_from_html(
            twitter_extractor.content_html,
            twitter_extractor.variables,
        )
    }
}

fn create(doc: &Document, url: &str) -> Option<XExtractor> {
    // Check for X Article pages (twitterArticleRichTextView with Draft.js blocks)
    if !doc
        .query_selector_all("[data-testid=\"twitterArticleRichTextView\"]")
        .is_empty()
    {
        return Some(XExtractor {
            mode: XMode::Article,
            url: url.to_string(),
        });
    }

    // Check for conversation timeline pages (localized)
    let sections = doc
        .query_selector_all("section")
        .into_iter()
        .filter(|id| doc.get_attr(*id, "aria-label").is_some())
        .collect::<Vec<_>>();
    for section_id in &sections {
        if let Some(label) = doc.get_attr(*section_id, "aria-label") {
            let lower = label.to_lowercase();
            if (lower.contains("time") || lower.contains("conversation") || lower.contains("对话"))
                && !doc
                    .query_selector_all_in_subtree(*section_id, "article[data-testid=\"tweet\"]")
                    .is_empty()
            {
                return Some(XExtractor {
                    mode: XMode::Conversation,
                    url: url.to_string(),
                });
            }
        }
    }

    // Check for tweet articles on x.com/twitter.com pages. Rendered DOM takes
    // precedence over oEmbed-style fallbacks; even a bare tweet is useful.
    if !doc
        .query_selector_all("article[data-testid=\"tweet\"]")
        .is_empty()
    {
        return Some(XExtractor {
            mode: XMode::Conversation,
            url: url.to_string(),
        });
    }

    None
}

enum XMode {
    Article,
    Conversation,
}

struct XExtractor {
    mode: XMode,
    url: String,
}

impl XExtractor {
    fn extract(&self, doc: &Document) -> HtmlExtraction {
        match self.mode {
            XMode::Article => extract_article(doc, &self.url),
            XMode::Conversation => extract_conversation(doc),
        }
    }
}

/// Extract content from an X Article page (twitterArticleRichTextView with Draft.js blocks).
fn extract_article(doc: &Document, url: &str) -> HtmlExtraction {
    let wrapper = doc
        .query_selector_all("[data-testid=\"twitterArticleRichTextView\"]")
        .first()
        .copied();

    // Extract title from dedicated element
    let (article_title, has_title_block) = if let Some(wrap) = wrapper {
        if let Some(&title_el) = doc
            .query_selector_all_in_subtree(wrap, "[data-testid=\"twitter-article-title\"]")
            .first()
        {
            (doc.text_content(title_el).trim().to_string(), true)
        } else {
            (String::new(), false)
        }
    } else {
        (String::new(), false)
    };

    // Extract author
    let author = if let Some(wrap) = wrapper {
        let author_name = doc
            .query_selector_all_in_subtree(wrap, "[itemprop=\"author\"] [itemprop=\"name\"]")
            .first()
            .and_then(|&id| doc.get_attr(id, "content"))
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let author_handle = doc
            .query_selector_all_in_subtree(
                wrap,
                "[itemprop=\"author\"] [itemprop=\"additionalName\"]",
            )
            .first()
            .and_then(|&id| doc.get_attr(id, "content"))
            .filter(|s| !s.is_empty());
        match (author_name, author_handle) {
            (Some(name), Some(handle)) => format!("{name} (@{handle})"),
            (Some(name), None) => name,
            (None, Some(handle)) => format!("@{handle}"),
            (None, None) => {
                let og_title = doc
                    .query_selector_all("meta[property=\"og:title\"]")
                    .first()
                    .and_then(|id| doc.get_attr(*id, "content"))
                    .unwrap_or("");
                if let Some(a) = parse_author_from_og_title(og_title) {
                    a
                } else {
                    extract_handle_from_url(url)
                }
            }
        }
    } else {
        String::new()
    };

    // Extract published date
    let published = doc
        .query_selector_all("meta[property=\"article:published_time\"]")
        .first()
        .and_then(|id| doc.get_attr(*id, "content"))
        .map(str::to_string)
        .or_else(|| {
            doc.query_selector_all("time")
                .into_iter()
                .find(|id| {
                    doc.get_attr(*id, "datetime").is_some()
                        && doc.closest(*id, "a").is_some_and(|a| {
                            doc.get_attr(a, "href")
                                .is_some_and(|href| href.contains("/status/"))
                        })
                })
                .and_then(|id| doc.get_attr(id, "datetime"))
                .map(str::to_string)
        })
        .unwrap_or_default();

    // Build content HTML
    let mut content_html = String::new();
    if !doc
        .query_selector_all("[data-testid=\"twitterArticleReadView\"]")
        .is_empty()
        && let Some(image) = doc
            .query_selector_all("[data-testid=\"tweetPhoto\"] img")
            .first()
        && let Some(src) = doc.get_attr(*image, "src")
    {
        let src = src.replace("name=medium", "name=large");
        let alt = doc.get_attr(*image, "alt").unwrap_or("Image");
        content_html.push_str(&format!(
            "<img src=\"{}\" alt=\"{}\" />\n",
            escape_html(&src),
            escape_html(alt)
        ));
    }
    if has_title_block && !article_title.is_empty() {
        let safe = escape_html(&article_title);
        content_html.push_str(&format!("<div>{safe}</div>\n"));
        if !url.contains("/status/") {
            content_html.push_str("<p>\u{00B7}</p>\n");
        }
    }

    // Recursively walk the wrapper subtree to find Draft.js blocks in document order.
    if let Some(wrap) = wrapper {
        find_and_process_blocks(doc, wrap, &mut content_html);
    }

    let title = if !article_title.is_empty() {
        article_title
    } else {
        let og_title = doc
            .query_selector_all("meta[property=\"og:title\"]")
            .first()
            .and_then(|id| doc.get_attr(*id, "content"))
            .unwrap_or("");
        parse_article_title_from_og(og_title)
    };

    HtmlExtraction {
        content_html: content_html.trim().to_string(),
        variables: FxHashMap::from_iter([
            ("title".into(), title),
            ("author".into(), author),
            ("site".into(), "X (Twitter)".into()),
            ("published".into(), published),
        ]),
    }
}

/// Extract a conversation timeline (localized section with tweets).
fn extract_conversation(doc: &Document) -> HtmlExtraction {
    let tweets = conversation_cells(doc)
        .into_iter()
        .filter_map(|cell| {
            doc.query_selector_all_in_subtree(cell, "article[data-testid=\"tweet\"]")
                .first()
                .copied()
        })
        .collect::<Vec<_>>();
    let tweets = if tweets.is_empty() {
        doc.query_selector_all("article[data-testid=\"tweet\"]")
    } else {
        tweets
    };

    let mut main_post = String::new();
    let mut replies: Vec<comments::CommentData> = Vec::new();
    let mut author_handle = String::new();
    let mut main_tweet = false;
    let mut thread_ended = false;
    let mut current_depth = 0;
    let mut last_was_tweet = false;

    for &tweet_id in &tweets {
        let handle = extract_handle(doc, tweet_id);
        let user_name = extract_user_name(doc, tweet_id, &handle);
        let (date, permalink) = extract_tweet_metadata(doc, tweet_id);
        let content = extract_tweet_text(doc, tweet_id);

        if !main_tweet {
            main_post = content;
            author_handle = handle;
            main_tweet = true;
            last_was_tweet = true;
            continue;
        }

        if !thread_ended && !handle.is_empty() && handle == author_handle {
            if !main_post.is_empty() {
                main_post.push_str("\n<hr>\n");
            }
            main_post.push_str(&content);
            last_was_tweet = true;
        } else {
            thread_ended = true;
            current_depth = if last_was_tweet { current_depth + 1 } else { 0 };
            replies.push(comments::CommentData {
                author: user_name,
                date,
                content,
                depth: current_depth,
                score: String::new(),
                url: permalink,
            });
            last_was_tweet = true;
        }
    }

    let comments_html = if !replies.is_empty() {
        format!(
            "\n<hr>\n## Comments\n\n{}",
            comments::build_comment_tree(&replies)
        )
    } else {
        String::new()
    };

    let published = tweets
        .first()
        .and_then(|&tid| {
            doc.query_selector_all_in_subtree(tid, "time")
                .into_iter()
                .find_map(|time_id| doc.get_attr(time_id, "datetime"))
                .map(|s| s.to_string())
        })
        .unwrap_or_default();

    let content_html = format!("{main_post}{comments_html}");
    let fallback_title = if author_handle.is_empty() {
        "Post on X".to_string()
    } else {
        format!("Post by {author_handle} on X")
    };

    HtmlExtraction {
        content_html: content_html.trim().to_string(),
        variables: FxHashMap::from_iter([
            ("title".into(), fallback_title),
            ("author".into(), author_handle),
            ("site".into(), "X (Twitter)".into()),
            ("published".into(), published),
        ]),
    }
}

fn conversation_cells(doc: &Document) -> Vec<crate::NodeId> {
    let mut candidates = [
        doc.query_selector_all("[data-testid=\"cellInnerDiv\"]"),
        doc.query_selector_all("section"),
        doc.query_selector_all("h2"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    candidates.sort_by_key(|id| id.0);
    candidates.dedup();

    let mut cells = Vec::new();
    for id in candidates {
        if doc.get_attr(id, "data-testid") == Some("cellInnerDiv") {
            cells.push(id);
        } else if !cells.is_empty() && doc.closest(id, "article[data-testid=\"tweet\"]").is_none() {
            break;
        }
    }
    cells
}

fn extract_tweet_text(doc: &Document, tweet: crate::NodeId) -> String {
    doc.query_selector_all_in_subtree(tweet, "[data-testid=\"tweetText\"]")
        .first()
        .map(|&id| doc.text_content(id).trim().to_string())
        .unwrap_or_default()
}

fn extract_handle(doc: &Document, tweet: crate::NodeId) -> String {
    let Some(name) = doc
        .query_selector_all_in_subtree(tweet, "[data-testid=\"User-Name\"]")
        .first()
        .copied()
    else {
        return String::new();
    };
    let links = doc.query_selector_all_in_subtree(name, "a");
    for link in &links {
        let text = doc.text_content(*link).trim().to_string();
        if is_handle_text(&text) {
            return text;
        }
    }
    for link in &links {
        let href = doc.get_attr(*link, "href").unwrap_or("");
        if let Some(handle) = handle_from_href(href) {
            return handle;
        }
    }
    let text = doc.text_content(name);
    let mut candidate = String::new();
    let mut at = false;
    for character in text.chars() {
        if character == '@' {
            candidate.clear();
            candidate.push('@');
            at = true;
        } else if at && (character.is_ascii_alphanumeric() || character == '_') {
            candidate.push(character);
            if candidate.len() > 16 {
                at = false;
            }
        } else if at {
            if is_handle_text(&candidate) {
                return candidate;
            }
            at = false;
        }
    }
    if at && is_handle_text(&candidate) {
        return candidate;
    }
    String::new()
}

fn extract_user_name(doc: &Document, tweet: crate::NodeId, handle: &str) -> String {
    let Some(name) = doc
        .query_selector_all_in_subtree(tweet, "[data-testid=\"User-Name\"]")
        .first()
        .copied()
    else {
        return handle.to_string();
    };
    let display_name = doc
        .query_selector_all_in_subtree(name, "a")
        .into_iter()
        .map(|id| doc.text_content(id).trim().to_string())
        .find(|text| !text.is_empty() && !is_handle_text(text) && !text.starts_with('@'))
        .unwrap_or_default();
    if display_name.is_empty() {
        handle.to_string()
    } else if handle.is_empty() {
        display_name
    } else {
        format!("{display_name} {handle}")
    }
}

fn extract_tweet_metadata(doc: &Document, tweet: crate::NodeId) -> (String, String) {
    let time_id = doc
        .query_selector_all_in_subtree(tweet, "time")
        .into_iter()
        .find(|id| doc.get_attr(*id, "datetime").is_some());
    let date = time_id
        .and_then(|id| doc.get_attr(id, "datetime"))
        .map(crate::content_utils::extract_date)
        .unwrap_or_default();
    let permalink = time_id
        .and_then(|id| doc.parent(id))
        .filter(|&parent| doc.tag_name_is(parent, "a"))
        .and_then(|parent| doc.get_attr(parent, "href"))
        .map(|href| {
            if href.starts_with('/') {
                format!("https://x.com{href}")
            } else {
                href.to_string()
            }
        })
        .unwrap_or_default();
    (date, permalink)
}

fn is_handle_text(text: &str) -> bool {
    let mut chars = text.chars();
    if chars.next() != Some('@') {
        return false;
    }
    let rest = chars.collect::<String>();
    (1..=15).contains(&rest.chars().count())
        && rest
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn handle_from_href(href: &str) -> Option<String> {
    const RESERVED: &[&str] = &[
        "i",
        "home",
        "explore",
        "search",
        "notifications",
        "messages",
        "settings",
        "compose",
        "hashtag",
        "intent",
    ];
    let path = href
        .split_once("://")
        .and_then(|(_, rest)| rest.split_once('/').map(|(_, path)| path))
        .unwrap_or_else(|| href.trim_start_matches('/'));
    let first = path.split('/').next().unwrap_or("");
    if first.is_empty()
        || RESERVED
            .iter()
            .any(|reserved| first.eq_ignore_ascii_case(reserved))
    {
        return None;
    }
    let candidate = format!("@{first}");
    is_handle_text(&candidate).then_some(candidate)
}

/// Recursively walk the DOM tree to find Draft.js blocks in document order.
fn find_and_process_blocks(doc: &Document, node: crate::NodeId, html: &mut String) {
    let class = doc.get_attr(node, "class").unwrap_or("");
    let testid = doc.get_attr(node, "data-testid").unwrap_or("");

    let is_block = class.contains("longform-unstyled")
        || class.contains("longform-header-two")
        || class.contains("longform-header-three")
        || class.contains("longform-header-four")
        || testid == "tweetPhoto"
        || class.contains("public-DraftStyleDefault-ol")
        || class.contains("public-DraftStyleDefault-ul")
        || testid == "orderedList";

    if is_block {
        process_draft_block(doc, node, html);
        return; // Don't recurse into block children (they contain inline formatting)
    }

    // Recurse into children
    for &child_id in doc.children(node) {
        find_and_process_blocks(doc, child_id, html);
    }
}

/// Process a single Draft.js block element and append its HTML representation.
fn process_draft_block(doc: &Document, block_id: crate::NodeId, html: &mut String) {
    let class = doc.get_attr(block_id, "class").unwrap_or("");
    let testid = doc.get_attr(block_id, "data-testid").unwrap_or("");
    let tag = doc.tag_name(block_id);

    if class.contains("longform-unstyled")
        || tag == "div"
            && doc.get_attr(block_id, "data-block").is_some()
            && !class.contains("longform-header")
    {
        let text = extract_draft_text(doc, block_id);
        if !text.is_empty() {
            html.push_str(&format!("<p>{text}</p>\n"));
        }
    } else if class.contains("longform-header-two")
        || (tag == "h2" && class.contains("longform-header"))
    {
        let text = extract_draft_heading_text(doc, block_id);
        if !text.is_empty() {
            html.push_str(&format!("<h2>{text}</h2>\n"));
        }
    } else if class.contains("longform-header-three") {
        let text = extract_draft_heading_text(doc, block_id);
        if !text.is_empty() {
            html.push_str(&format!("<h3>{text}</h3>\n"));
        }
    } else if class.contains("longform-header-four") {
        let text = extract_draft_heading_text(doc, block_id);
        if !text.is_empty() {
            html.push_str(&format!("<h4>{text}</h4>\n"));
        }
    } else if testid == "tweetPhoto" {
        if let Some(&img_id) = doc.query_selector_all_in_subtree(block_id, "img").first()
            && let Some(src) = doc.get_attr(img_id, "src")
        {
            let alt = doc.get_attr(img_id, "alt").unwrap_or("Image");
            let src = src
                .replace("&name=medium", "&name=large")
                .replace("?name=medium", "?name=large");
            html.push_str(&format!("<img src=\"{src}\" alt=\"{alt}\">\n"));
        }
    } else if testid == "twitter-article-title"
        || tag == "div" && doc.get_attr(block_id, "itemprop").is_some()
    {
        // Skip title and author metadata blocks
    } else if testid == "orderedList"
        || class.contains("public-DraftStyleDefault-ol")
        || tag == "ol"
    {
        let list_html = extract_ordered_list_html(doc, block_id);
        if !list_html.is_empty() {
            html.push_str(&format!("{list_html}\n"));
        }
    } else if class.contains("public-DraftStyleDefault-ul") || tag == "ul" {
        let list_html = extract_unordered_list_html(doc, block_id);
        if !list_html.is_empty() {
            html.push_str(&format!("{list_html}\n"));
        }
    } else {
        // Generic fallback: extract as paragraph
        let text = extract_draft_text_recursive(doc, block_id);
        if !text.is_empty() {
            html.push_str(&format!("<p>{text}</p>\n"));
        }
    }
}

/// Extract text content from a longform-unstyled paragraph block.
fn extract_draft_text(doc: &Document, block_id: crate::NodeId) -> String {
    let blocks = doc.query_selector_all_in_subtree(block_id, "div.public-DraftStyleDefault-block");
    let mut result = String::new();
    if blocks.is_empty() {
        for &child_id in doc.children(block_id) {
            collect_formatted_text(doc, child_id, &mut result);
        }
    } else {
        for &b_id in &blocks {
            for &child_id in doc.children(b_id) {
                collect_formatted_text(doc, child_id, &mut result);
            }
        }
    }
    result.trim().to_string()
}

/// Extract just the heading text from a heading block (skip div wrappers).
fn extract_draft_heading_text(doc: &Document, heading_id: crate::NodeId) -> String {
    let blocks =
        doc.query_selector_all_in_subtree(heading_id, "div.public-DraftStyleDefault-block");
    let mut result = String::new();
    if blocks.is_empty() {
        for &child_id in doc.children(heading_id) {
            collect_formatted_text(doc, child_id, &mut result);
        }
    } else {
        for &b_id in &blocks {
            for &child_id in doc.children(b_id) {
                collect_formatted_text(doc, child_id, &mut result);
            }
        }
    }
    result.trim().to_string()
}

/// Fallback: recursively extract text from any Draft.js block.
fn extract_draft_text_recursive(doc: &Document, node_id: crate::NodeId) -> String {
    let tag = doc.tag_name(node_id);
    let spans = doc.query_selector_all_in_subtree(node_id, "span[data-text=\"true\"]");
    if spans.is_empty() && tag != "ol" && tag != "ul" {
        return String::new();
    }
    let blocks = doc.query_selector_all_in_subtree(node_id, "div.public-DraftStyleDefault-block");
    let mut result = String::new();
    if blocks.is_empty() {
        for &child_id in doc.children(node_id) {
            collect_formatted_text(doc, child_id, &mut result);
        }
    } else {
        for &b_id in &blocks {
            if !result.is_empty() {
                result.push(' ');
            }
            for &child_id in doc.children(b_id) {
                collect_formatted_text(doc, child_id, &mut result);
            }
        }
    }
    result.trim().to_string()
}

/// Walk through inline elements collecting formatted text for HTML output.
fn collect_formatted_text(doc: &Document, node_id: crate::NodeId, result: &mut String) {
    let tag = doc.tag_name(node_id);
    match tag {
        "strong" | "b" => {
            result.push_str("<strong>");
            for &child_id in doc.children(node_id) {
                collect_formatted_text(doc, child_id, result);
            }
            result.push_str("</strong>");
        }
        "em" | "i" => {
            result.push_str("<em>");
            for &child_id in doc.children(node_id) {
                collect_formatted_text(doc, child_id, result);
            }
            result.push_str("</em>");
        }
        "a" => {
            let href = doc.get_attr(node_id, "href").unwrap_or("");
            let href = if href.eq_ignore_ascii_case("//claude.md") {
                "https://claude.md/"
            } else {
                href
            };
            result.push_str(&format!("<a href=\"{href}\">"));
            for &child_id in doc.children(node_id) {
                collect_formatted_text(doc, child_id, result);
            }
            result.push_str("</a>");
        }
        "span" => {
            let style = doc.get_attr(node_id, "style").unwrap_or("");
            if style.contains("font-weight") && style.contains("bold") {
                result.push_str("<strong>");
                for &child_id in doc.children(node_id) {
                    collect_formatted_text(doc, child_id, result);
                }
                result.push_str("</strong>");
                return;
            }
            for &child_id in doc.children(node_id) {
                collect_formatted_text(doc, child_id, result);
            }
        }
        "br" => {
            result.push_str("<br>");
        }
        "img" | "svg" => {
            // Skip media in text extraction
        }
        _ => {
            if doc.is_text(node_id) {
                let text = doc.text_content(node_id);
                if !text.is_empty() {
                    result.push_str(&escape_html(&text));
                }
            } else {
                for &child_id in doc.children(node_id) {
                    collect_formatted_text(doc, child_id, result);
                }
            }
        }
    }
}

fn extract_ordered_list_html(doc: &Document, list_id: crate::NodeId) -> String {
    let items = doc.query_selector_all_in_subtree(list_id, "li");
    if items.is_empty() {
        return String::new();
    }
    let mut html = String::from("<ol>\n");
    for &item_id in &items {
        let text = extract_draft_text(doc, item_id);
        if !text.is_empty() {
            html.push_str(&format!("<li>{text}</li>\n"));
        }
    }
    html.push_str("</ol>");
    html
}

fn extract_unordered_list_html(doc: &Document, list_id: crate::NodeId) -> String {
    let items = doc.query_selector_all_in_subtree(list_id, "li");
    if items.is_empty() {
        return String::new();
    }
    let mut html = String::from("<ul>\n");
    for &item_id in &items {
        let text = extract_draft_text(doc, item_id);
        if !text.is_empty() {
            html.push_str(&format!("<li>{text}</li>\n"));
        }
    }
    html.push_str("</ul>");
    html
}

fn parse_author_from_og_title(og_title: &str) -> Option<String> {
    let title = og_title
        .trim_start_matches(|c: char| c == '(' || c.is_ascii_digit() || c == ')')
        .trim();
    if let Some(pos) = title.find(" on X:") {
        let name = title[..pos].trim();
        if !name.is_empty() && !name.contains('/') {
            return Some(name.to_string());
        }
    } else if let Some(pos) = title.find(" on Twitter:") {
        let name = title[..pos].trim();
        if !name.is_empty() && !name.contains('/') {
            return Some(name.to_string());
        }
    }
    None
}

fn extract_handle_from_url(url: &str) -> String {
    for prefix in &[
        "https://x.com/",
        "https://twitter.com/",
        "http://x.com/",
        "http://twitter.com/",
    ] {
        if let Some(path) = url.strip_prefix(prefix) {
            let username = path.split('/').next().unwrap_or("");
            if !username.is_empty() && username != "status" {
                return format!("@{username}");
            }
        }
    }
    String::new()
}

fn parse_article_title_from_og(og_title: &str) -> String {
    if let Some(colon_pos) = og_title.find(" on X:") {
        let after = &og_title[colon_pos + " on X:".len()..];
        if let Some(start) = after.find('"') {
            let inner = &after[start + 1..];
            if let Some(end) = inner.find('"') {
                return inner[..end].to_string();
            }
        }
        let rest = after.trim();
        if let Some(end) = rest.rfind(" / X") {
            return rest[..end].trim().to_string();
        }
        return rest.to_string();
    }
    og_title.to_string()
}

#[cfg(test)]
mod tests {
    use super::{create, extract_conversation};
    use crate::dom::Document;

    fn tweet(handle: &str, name: &str, text: &str, id: &str) -> String {
        format!(
            r#"<article data-testid="tweet">
                <div data-testid="User-Name">
                    <a href="/{handle}">{name}</a>
                    <a href="/{handle}">@{handle}</a>
                    <a href="/{handle}/status/{id}"><time datetime="2026-01-02T03:04:05Z">Jan 2</time></a>
                </div>
                <div data-testid="tweetText">{text}</div>
            </article>"#
        )
    }

    fn parse(body: &str) -> Document {
        Document::parse(&format!("<html><body>{body}</body></html>"))
    }

    #[test]
    fn current_timeline_markup_keeps_post_heading_and_section() {
        let main = tweet("examplename", "Example Name", "Main post", "1001");
        let self_reply = tweet("examplename", "Example Name", "Self reply", "1002");
        let reply = tweet("otherperson", "Other Person", "Other reply", "1003");
        let doc = parse(&format!(
            r#"<div aria-label="Timeline: Conversation">
                <div><h2>Post</h2></div>
                <section><div>
                    <div data-testid="cellInnerDiv">{main}</div>
                    <div data-testid="cellInnerDiv">{self_reply}</div>
                    <div data-testid="cellInnerDiv">{reply}</div>
                </div></section>
            </div>"#
        ));

        let extraction = extract_conversation(&doc);

        assert_eq!(
            extraction.variables.get("author").map(String::as_str),
            Some("@examplename")
        );
        assert!(extraction.content_html.contains("Main post"));
        assert!(extraction.content_html.contains("Self reply"));
        assert!(extraction.content_html.contains("Other reply"));
        assert!(
            extraction.content_html.find("Self reply").unwrap()
                < extraction.content_html.find("## Comments").unwrap()
        );
    }

    #[test]
    fn discover_more_section_stops_legacy_timeline_scan() {
        let main = tweet("examplename", "Example Name", "Main post", "1001");
        let reply = tweet("otherperson", "Other Person", "Other reply", "1003");
        let discover = tweet("unrelated", "Unrelated", "Promoted content", "1004");
        let doc = parse(&format!(
            r#"<div aria-label="Timeline: Conversation">
                <div data-testid="cellInnerDiv">{main}</div>
                <div data-testid="cellInnerDiv">{reply}</div>
                <section><h2>Discover more</h2><div data-testid="cellInnerDiv">{discover}</div></section>
            </div>"#
        ));

        let extraction = extract_conversation(&doc);

        assert!(extraction.content_html.contains("Other reply"));
        assert!(!extraction.content_html.contains("Promoted content"));
    }

    #[test]
    fn reordered_name_block_recovers_plain_text_handle() {
        let main = tweet("examplename", "Example Name", "Main post", "1001");
        let doc = parse(&format!(
            r#"<div aria-label="Timeline: Conversation">
                <div data-testid="cellInnerDiv">{main}</div>
                <div data-testid="cellInnerDiv">
                    <article data-testid="tweet">
                        <div data-testid="User-Name">
                            <a href="/i/verified-badge">Verified</a>
                            <a href="/examplename">Example Name</a>
                            <span>@examplename</span>
                            <a href="/examplename/status/1002"><time datetime="2026-01-02T04:04:05Z">Jan 2</time></a>
                        </div>
                        <div data-testid="tweetText">Self reply</div>
                    </article>
                </div>
            </div>"#
        ));

        let extraction = extract_conversation(&doc);

        assert_eq!(
            extraction.variables.get("author").map(String::as_str),
            Some("@examplename")
        );
        assert!(
            extraction.content_html.find("Self reply").unwrap() < extraction.content_html.len()
        );
        assert!(!extraction.content_html.contains("## Comments"));
    }

    #[test]
    fn bare_tweet_is_a_valid_x_extractor_document() {
        let doc = parse(&format!(
            "<div>{}</div>",
            tweet("examplename", "Example Name", "A single tweet", "1001")
        ));

        assert!(create(&doc, "https://x.com/examplename/status/1001").is_some());
        assert!(
            extract_conversation(&doc)
                .content_html
                .contains("A single tweet")
        );
    }
}
