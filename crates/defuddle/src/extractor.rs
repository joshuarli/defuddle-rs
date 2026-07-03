use std::collections::BTreeMap;

use crate::dom::Document;

pub struct ExtractorResult {
    pub content_html: String,
    pub content_selector: Option<String>,
    pub variables: BTreeMap<String, String>,
}

pub trait Extractor {
    fn can_extract(&self, doc: &Document) -> bool;
    fn extract(&self, doc: &Document) -> ExtractorResult;
}

type ExtractorFactory = fn(doc: &Document, url: &str) -> Option<Box<dyn Extractor>>;

pub struct Registry {
    mappings: Vec<(String, ExtractorFactory)>,
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            mappings: Vec::new(),
        }
    }

    pub fn register(&mut self, domain: &str, factory: ExtractorFactory) {
        self.mappings.push((domain.to_string(), factory));
    }

    pub fn find(&self, url: &str, doc: &Document) -> Option<Box<dyn Extractor>> {
        let hostname = extract_hostname(url);
        for (domain, factory) in &self.mappings {
            if hostname.contains(domain.as_str()) || url.contains(domain.as_str()) {
                if let Some(extractor) = factory(doc, url) {
                    if extractor.can_extract(doc) {
                        return Some(extractor);
                    }
                }
            }
        }
        None
    }
}

fn extract_hostname(url: &str) -> String {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    stripped
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_lowercase()
}

pub fn build_registry() -> Registry {
    let mut registry = Registry::new();

    registry.register("news.ycombinator.com", hackernews::create);
    registry.register("old.reddit.com", reddit::create);
    registry.register("reddit.com", reddit::create);
    registry.register("github.com", github::create);
    registry.register("chatgpt.com", chatgpt::create);
    registry.register("substack.com", substack::create);
    registry.register("mastodon", mastodon::create);
    registry.register("store.example.com", bbcode_data::create);

    registry
}

pub mod comments {
    pub struct CommentData {
        pub author: String,
        pub date: String,
        pub content: String,
        pub depth: u32,
        pub score: String,
        pub url: String,
    }

    pub fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }

    pub fn build_comment(comment: &CommentData) -> String {
        let author = format!(
            "<span class=\"comment-author\"><strong>{}</strong></span>",
            escape_html(&comment.author)
        );
        let date_html = if !comment.url.is_empty() {
            format!(
                "<a href=\"{}\" class=\"comment-link\">{}</a>",
                escape_html(&comment.url),
                escape_html(&comment.date)
            )
        } else {
            format!(
                "<span class=\"comment-date\">{}</span>",
                escape_html(&comment.date)
            )
        };
        let score_html = if !comment.score.is_empty() {
            format!(
                " · <span class=\"comment-points\">{}</span>",
                escape_html(&comment.score)
            )
        } else {
            String::new()
        };

        format!(
            "<div class=\"comment\">\n\t<div class=\"comment-metadata\">\n\t\t{} · {}{}\n\t</div>\n\t<div class=\"comment-content\">{}</div>\n</div>",
            author, date_html, score_html, comment.content
        )
    }

    pub fn build_comment_tree(comments: &[CommentData]) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut blockquote_stack: Vec<u32> = Vec::new();

        for comment in comments {
            let depth = comment.depth;
            if depth == 0 {
                while !blockquote_stack.is_empty() {
                    parts.push("</blockquote>".into());
                    blockquote_stack.pop();
                }
                parts.push("<blockquote>".into());
                blockquote_stack.push(0);
            } else {
                let current = blockquote_stack.last().copied().unwrap_or(0);
                if depth < current {
                    while blockquote_stack.last().copied().unwrap_or(0) >= depth {
                        parts.push("</blockquote>".into());
                        blockquote_stack.pop();
                    }
                }
                let new_current = blockquote_stack.last().copied().unwrap_or(0);
                if depth > new_current {
                    parts.push("<blockquote>".into());
                    blockquote_stack.push(depth);
                }
            }

            parts.push(build_comment(comment));
        }

        while !blockquote_stack.is_empty() {
            parts.push("</blockquote>".into());
            blockquote_stack.pop();
        }

        parts.join("")
    }

    pub fn build_content_html(site: &str, post_content: &str, comments: &str) -> String {
        let comments_section = if comments.is_empty() {
            String::new()
        } else {
            format!(
                "\n\t\t\t<hr>\n\t\t\t<div class=\"{site} comments\">\n\t\t\t\t<h2>Comments</h2>\n\t\t\t\t{comments}\n\t\t\t</div>"
            )
        };

        format!(
            "<article data-defuddle>\n\t\t\t<div class=\"{site} post\">\n\t\t\t\t<div class=\"post-content\">\n\t\t\t\t\t{post_content}\n\t\t\t\t</div>\n\t\t\t</div>{}</article>",
            comments_section
        )
    }
}

pub mod hackernews {
    use super::*;

    pub fn create(doc: &Document, url: &str) -> Option<Box<dyn Extractor>> {
        let main_post = doc.query_selector_all(".fatitem");
        let is_listing = !doc.query_selector_all("tr.athing").is_empty() && main_post.is_empty();

        if main_post.is_empty() && !is_listing {
            return None;
        }

        let is_comment_page = main_post.first().map_or(false, |&pid| {
            !doc.query_selector_all(".onstory").is_empty()
                && doc.query_selector_all(".titleline").is_empty()
        });

        Some(Box::new(HackerNewsExtractor {
            has_main_post: !main_post.is_empty(),
            is_listing,
            is_comment_page,
        }))
    }

    struct HackerNewsExtractor {
        has_main_post: bool,
        is_listing: bool,
        is_comment_page: bool,
    }

    impl Extractor for HackerNewsExtractor {
        fn can_extract(&self, _doc: &Document) -> bool {
            self.has_main_post || self.is_listing
        }

        fn extract(&self, doc: &Document) -> ExtractorResult {
            if self.is_listing {
                return ExtractorResult {
                    content_html: String::new(),
                    content_selector: None,
                    variables: BTreeMap::from([
                        ("title".into(), "Hacker News".into()),
                        ("site".into(), "Hacker News".into()),
                    ]),
                };
            }
            self.extract_post(doc)
        }
    }

    impl HackerNewsExtractor {
        fn extract_post(&self, doc: &Document) -> ExtractorResult {
            let post_content = self.get_post_content(doc);
            let comments = self.extract_comments(doc);
            let content_html = super::comments::build_content_html("hackernews", &post_content, &comments);

            let post_title = self.get_post_title(doc);
            let post_author = self.get_post_author(doc);
            let description = format!("{} - by {} on Hacker News", post_title, post_author);
            let published = self.get_post_date(doc);

            ExtractorResult {
                content_html,
                content_selector: None,
                variables: BTreeMap::from([
                    ("title".into(), post_title),
                    ("author".into(), post_author),
                    ("site".into(), "Hacker News".into()),
                    ("description".into(), description),
                    ("published".into(), published),
                ]),
            }
        }

        fn get_post_title(&self, doc: &Document) -> String {
            doc.query_selector_all(".titleline")
                .first()
                .map(|id| doc.text_content(*id).trim().to_string())
                .unwrap_or_default()
        }

        fn get_post_author(&self, doc: &Document) -> String {
            doc.query_selector_all(".hnuser")
                .first()
                .map(|id| doc.text_content(*id).trim().to_string())
                .unwrap_or_default()
        }

        fn get_post_date(&self, doc: &Document) -> String {
            doc.query_selector_all(".age")
                .first()
                .and_then(|id| doc.get_attr(*id, "title").map(|s| s.to_string()))
                .map(|t| t.split('T').next().unwrap_or("").to_string())
                .unwrap_or_default()
        }

        fn get_post_content(&self, doc: &Document) -> String {
            if self.is_comment_page {
                if let Some(comment_id) = doc.query_selector_all("tr.athing").first().copied() {
                    let author = doc.query_selector_all(".hnuser")
                        .first()
                        .map(|id| doc.text_content(*id).trim().to_string())
                        .unwrap_or_else(|| "[deleted]".into());
                    let commtext = doc.query_selector_all(".commtext")
                        .first()
                        .map(|id| doc.inner_html(*id))
                        .unwrap_or_default();
                    let points = doc.query_selector_all(".score")
                        .first()
                        .map(|id| doc.text_content(*id).trim().to_string())
                        .unwrap_or_default();
                    return super::comments::build_comment(&super::comments::CommentData {
                        author,
                        date: self.get_post_date(doc),
                        content: commtext,
                        depth: 0,
                        score: points,
                        url: String::new(),
                    });
                }
                return String::new();
            }

            let mut content = String::new();
            let titleline_url = doc.query_selector_all(".titleline a")
                .first()
                .and_then(|id| doc.get_attr(*id, "href").map(|s| s.to_string()))
                .unwrap_or_default();
            if !titleline_url.is_empty() {
                content.push_str(&format!(
                    "<p><a href=\"{}\" target=\"_blank\">{}</a></p>",
                    super::comments::escape_html(&titleline_url),
                    super::comments::escape_html(&titleline_url)
                ));
            }
            if let Some(toptext_id) = doc.query_selector_all(".toptext").first().copied() {
                content.push_str(&format!(
                    "<div class=\"post-text\">{}</div>",
                    doc.inner_html(toptext_id)
                ));
            }
            content
        }

        fn extract_comments(&self, doc: &Document) -> String {
            let comtrs = doc.query_selector_all("tr.comtr");
            let mut comment_data = Vec::new();
            let mut processed = std::collections::BTreeSet::new();

            for comtr in comtrs {
                let id_attr = doc.get_attr(comtr, "id").map(|s| s.to_string()).unwrap_or_default();
                if id_attr.is_empty() || processed.contains(&id_attr) {
                    continue;
                }
                processed.insert(id_attr.clone());

                let indent_width = doc.query_selector_all(".ind img")
                    .first()
                    .and_then(|id| doc.get_attr(*id, "width"))
                    .and_then(|w| w.parse::<usize>().ok())
                    .unwrap_or(0);
                let depth = indent_width / 40;

                let author = doc.query_selector_all(".hnuser")
                    .first()
                    .map(|id| doc.text_content(*id).trim().to_string())
                    .unwrap_or_else(|| "[deleted]".into());

                let commtext = doc.query_selector_all(".commtext")
                    .first()
                    .map(|id| doc.inner_html(*id))
                    .unwrap_or_default();

                let points = doc.query_selector_all(".score")
                    .first()
                    .map(|id| doc.text_content(*id).trim().to_string())
                    .unwrap_or_default();

                let age_title = doc.query_selector_all(".age")
                    .first()
                    .and_then(|id| doc.get_attr(*id, "title").map(|s| s.to_string()))
                    .unwrap_or_default();
                let date = age_title.split('T').next().unwrap_or("").to_string();

                let comment_url = format!("https://news.ycombinator.com/item?id={id_attr}");

                comment_data.push(super::comments::CommentData {
                    author,
                    date,
                    content: commtext,
                    depth: depth as u32,
                    score: points,
                    url: comment_url,
                });
            }

            super::comments::build_comment_tree(&comment_data)
        }
    }
}

pub mod reddit {
    use super::*;

    pub fn create(doc: &Document, url: &str) -> Option<Box<dyn Extractor>> {
        let is_old = !doc.query_selector_all(".thing.link").is_empty();
        if is_old {
            Some(Box::new(RedditExtractor {
                url: url.to_string(),
            }))
        } else {
            None
        }
    }

    struct RedditExtractor {
        url: String,
    }

    impl Extractor for RedditExtractor {
        fn can_extract(&self, _doc: &Document) -> bool {
            true
        }

        fn extract(&self, doc: &Document) -> ExtractorResult {
            let post_title = doc.query_selector_all("a.title")
                .first()
                .map(|id| doc.text_content(*id).trim().to_string())
                .unwrap_or_default();

            let post_author = doc.query_selector_all(".thing.link")
                .first()
                .and_then(|id| doc.get_attr(*id, "data-author"))
                .unwrap_or("")
                .to_string();
            let subreddit = doc.query_selector_all(".thing.link")
                .first()
                .and_then(|id| doc.get_attr(*id, "data-subreddit"))
                .unwrap_or("")
                .to_string();

            let post_body = doc.query_selector_all(".usertext-body .md")
                .first()
                .map(|id| doc.inner_html(*id))
                .unwrap_or_default();

            let comments = collect_old_comments(doc);
            let comments_html = if !comments.is_empty() {
                super::comments::build_comment_tree(&comments)
            } else {
                String::new()
            };

            let content_html = super::comments::build_content_html("reddit", &post_body, &comments_html);
            let post_id = self.url
                .split("/comments/")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .unwrap_or("")
                .to_string();

            ExtractorResult {
                content_html,
                content_selector: None,
                variables: BTreeMap::from([
                    ("title".into(), post_title),
                    ("author".into(), post_author),
                    ("site".into(), format!("r/{subreddit}")),
                ]),
            }
        }
    }

    fn collect_old_comments(doc: &Document) -> Vec<super::comments::CommentData> {
        let mut result = Vec::new();
        let commentarea = doc.query_selector_all(".commentarea .sitetable");
        if let Some(container) = commentarea.first().copied() {
            collect_comments_recursive(doc, container, 0, &mut result);
        }
        result
    }

    fn collect_comments_recursive(
        doc: &Document,
        container: crate::dom::NodeId,
        depth: u32,
        result: &mut Vec<super::comments::CommentData>,
    ) {
        if depth > 20 {
            return;
        }
        let thing_comments: Vec<_> = doc.query_selector_all(".thing.comment")
            .into_iter()
            .filter(|&c| doc.contains(container, c))
            .collect();
        let mut seen_children = std::collections::BTreeSet::new();
        for comment in thing_comments {
            if !seen_children.insert(comment) {
                continue;
            }
            let author = doc.get_attr(comment, "data-author").unwrap_or("").to_string();
            let permalink = doc.get_attr(comment, "data-permalink").unwrap_or("").to_string();
            let score_text = doc.query_selector_all(".entry .tagline .score.unvoted")
                .first()
                .map(|id| doc.text_content(*id).trim().to_string())
                .unwrap_or_default();
            let time_el = doc.query_selector_all(".entry .tagline time[datetime]")
                .first()
                .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string()))
                .unwrap_or_default();
            let date = time_el.split('T').next().unwrap_or("").to_string();
            let body_html = doc.query_selector_all(".entry .usertext-body .md")
                .iter()
                .find(|&&el| doc.contains(comment, el))
                .map(|&el| doc.inner_html(el))
                .unwrap_or_default();

            let url = if !permalink.is_empty() {
                format!("https://reddit.com{permalink}")
            } else {
                String::new()
            };

            result.push(super::comments::CommentData {
                author,
                date,
                content: body_html,
                depth,
                score: score_text,
                url,
            });

            let children: Vec<_> = doc.query_selector_all(".child > .sitetable")
                .into_iter()
                .filter(|&c| doc.contains(comment, c))
                .collect();
            for child in children {
                collect_comments_recursive(doc, child, depth + 1, result);
            }
        }
    }
}

pub mod github {
    use super::*;

    pub fn create(doc: &Document, url: &str) -> Option<Box<dyn Extractor>> {
        let is_pr = url.contains("/pull/");
        let is_issue = url.contains("/issues/");

        if !is_pr && !is_issue {
            return None;
        }

        let indicators = [
            "meta[name=\"expected-hostname\"][content=\"github.com\"]",
            ".js-header-wrapper",
            "#js-repo-pjax-container",
        ];
        let has_indicator = indicators.iter().any(|sel| !doc.query_selector_all(sel).is_empty());

        if !has_indicator {
            return None;
        }

        let repo_info = extract_repo_info(url);
        let number = extract_number(url);

        Some(Box::new(GitHubExtractor {
            repo_info,
            number,
        }))
    }

    fn extract_repo_info(url: &str) -> (String, String) {
        let parts: Vec<&str> = url.split('/').collect();
        let owner = parts.get(3).map(|s| s.to_string()).unwrap_or_default();
        let repo = parts.get(4).map(|s| s.to_string()).unwrap_or_default();
        (owner, repo)
    }

    fn extract_number(url: &str) -> String {
        let parts: Vec<&str> = url.split('/').collect();
        parts.get(5).map(|s| s.to_string()).unwrap_or_default()
    }

    struct GitHubExtractor {
        repo_info: (String, String),
        number: String,
    }

    impl Extractor for GitHubExtractor {
        fn can_extract(&self, _doc: &Document) -> bool {
            true
        }

        fn extract(&self, doc: &Document) -> ExtractorResult {
            let (owner, repo) = &self.repo_info;

            let title = doc.query_selector_all("title")
                .first()
                .map(|id| doc.text_content(*id).trim().to_string())
                .unwrap_or_default();

            let post_content = doc.query_selector_all(".js-comment-body, .comment-body.markdown-body")
                .first()
                .map(|id| doc.inner_html(*id))
                .unwrap_or_default();

            let content_html = super::comments::build_content_html("github", &post_content, "");

            ExtractorResult {
                content_html,
                content_selector: None,
                variables: BTreeMap::from([
                    ("title".into(), title),
                    ("site".into(), format!("GitHub - {owner}/{repo}")),
                    ("author".into(), String::new()),
                    ("published".into(), String::new()),
                    ("description".into(), String::new()),
                ]),
            }
        }
    }
}

pub mod chatgpt {
    use super::*;

    pub fn create(doc: &Document, _url: &str) -> Option<Box<dyn Extractor>> {
        if doc.query_selector_all("[data-testid^=\"conversation-turn-\"]").is_empty() {
            return None;
        }
        Some(Box::new(ChatGPTExtractor))
    }

    struct ChatGPTExtractor;

    impl Extractor for ChatGPTExtractor {
        fn can_extract(&self, _doc: &Document) -> bool {
            true
        }

        fn extract(&self, doc: &Document) -> ExtractorResult {
            let title = doc.query_selector_all("title")
                .first()
                .map(|id| doc.text_content(*id).trim().to_string())
                .unwrap_or_else(|| "ChatGPT Conversation".into());

            let turns = doc.query_selector_all("[data-testid^=\"conversation-turn-\"]");
            let mut content_parts: Vec<String> = Vec::new();

            for turn_id in turns {
                let heading = doc.query_selector_all("h4,h5,h6")
                    .iter()
                    .find(|&&h| doc.contains(turn_id, h))
                    .map(|&h| doc.text_content(h).trim().trim_end_matches(':').to_string())
                    .unwrap_or_default();

                if !heading.is_empty() {
                    content_parts.push(format!("**{heading} said**"));
                    content_parts.push(String::new());
                }

                let msg_html = doc.query_selector_all(".markdown, .whitespace-pre-wrap")
                    .iter()
                    .find(|&&el| doc.contains(turn_id, el))
                    .map(|&el| doc.inner_html(el))
                    .unwrap_or_else(|| doc.inner_html(turn_id));

                if !msg_html.trim().is_empty() {
                    content_parts.push(msg_html);
                }

                content_parts.push(String::new());
            }

            let content = content_parts.join("\n").trim().to_string();

            ExtractorResult {
                content_html: content,
                content_selector: None,
                variables: BTreeMap::from([
                    ("title".into(), title),
                    ("site".into(), "ChatGPT".into()),
                    ("author".into(), String::new()),
                    ("published".into(), String::new()),
                ]),
            }
        }
    }
}

pub mod substack {
    use super::*;

    pub fn create(doc: &Document, url: &str) -> Option<Box<dyn Extractor>> {
        let has_note = url.contains("/note/")
            || !doc.query_selector_all(".ProseMirror, .FeedProseMirror").is_empty();

        if !has_note {
            return None;
        }
        Some(Box::new(SubstackExtractor))
    }

    struct SubstackExtractor;

    impl Extractor for SubstackExtractor {
        fn can_extract(&self, _doc: &Document) -> bool {
            true
        }

        fn extract(&self, doc: &Document) -> ExtractorResult {
            let title = doc.query_selector_all("meta[property=\"og:title\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .unwrap_or("")
                .to_string();

            let author = title
                .rsplit(" (@")
                .last()
                .unwrap_or(&title)
                .trim()
                .to_string();

            let note_html = doc.query_selector_all(".ProseMirror, .FeedProseMirror")
                .first()
                .map(|id| doc.inner_html(*id))
                .unwrap_or_default();

            ExtractorResult {
                content_html: note_html,
                content_selector: None,
                variables: BTreeMap::from([
                    ("title".into(), title),
                    ("author".into(), author),
                    ("site".into(), "Substack".into()),
                ]),
            }
        }
    }
}

pub mod mastodon {
    use super::*;

    pub fn create(doc: &Document, _url: &str) -> Option<Box<dyn Extractor>> {
        if doc.query_selector_all(".detailed-status__wrapper, .status__wrapper").is_empty() {
            return None;
        }
        Some(Box::new(MastodonExtractor))
    }

    struct MastodonExtractor;

    impl Extractor for MastodonExtractor {
        fn can_extract(&self, _doc: &Document) -> bool {
            true
        }

        fn extract(&self, doc: &Document) -> ExtractorResult {
            let main_post = doc.query_selector_all(".detailed-status__wrapper")
                .first()
                .copied();

            let main_handle = main_post.and_then(|mp| {
                doc.query_selector_all(".display-name__account")
                    .iter()
                    .find(|&&h| doc.contains(mp, h))
                    .map(|&h| doc.text_content(h).trim().trim_start_matches('@').to_string())
            }).unwrap_or_default();

            let content_html = if let Some(main_id) = main_post {
                let text = doc.query_selector_all(".status__content__text")
                    .iter()
                    .find(|&&s| doc.contains(main_id, s))
                    .map(|&s| doc.inner_html(s).trim().to_string())
                    .unwrap_or_default();

                let mut post_parts: Vec<String> = Vec::new();
                if !text.is_empty() { post_parts.push(text); }

                // Classify reply wrappers: self-replies form thread, others form comments
                let reply_wrappers: Vec<_> = doc.query_selector_all(".status__wrapper")
                    .into_iter()
                    .filter(|&w| w != main_id && doc.query_selector_all(".status[data-id]").iter().any(|&s| doc.contains(w, s)))
                    .collect();

                let mut thread_ended = false;
                let mut comment_wrappers = Vec::new();

                for &wrapper in &reply_wrappers {
                    let wrapper_handle = doc.query_selector_all(".display-name__account")
                        .iter()
                        .find(|&&h| doc.contains(wrapper, h))
                        .map(|&h| doc.text_content(h).trim().trim_start_matches('@').to_string())
                        .unwrap_or_default();

                    if !thread_ended && wrapper_handle == main_handle {
                        let thread_text = doc.query_selector_all(".status__content__text")
                            .iter()
                            .find(|&&s| doc.contains(wrapper, s))
                            .map(|&s| doc.inner_html(s).trim().to_string())
                            .unwrap_or_default();
                        if !thread_text.is_empty() {
                            post_parts.push(thread_text);
                        }
                    } else {
                        thread_ended = true;
                        comment_wrappers.push(wrapper);
                    }
                }

                let post_content = post_parts.join("\n<hr>\n");

                let mut replies = Vec::new();
                for &wrapper in &comment_wrappers {
                    let handle = doc.query_selector_all(".display-name__account")
                        .iter()
                        .find(|&&h| doc.contains(wrapper, h))
                        .map(|&h| doc.text_content(h).trim().trim_start_matches('@').to_string())
                        .unwrap_or_default();

                    let display_name = doc.query_selector_all(".display-name__html")
                        .iter()
                        .find(|&&dn| doc.contains(wrapper, dn))
                        .map(|&dn| doc.text_content(dn).trim().to_string())
                        .unwrap_or_default();

                    let reply_content = doc.query_selector_all(".status__content__text")
                        .iter()
                        .find(|&&rc| doc.contains(wrapper, rc))
                        .map(|&rc| doc.inner_html(rc).trim().to_string())
                        .unwrap_or_default();

                    if reply_content.is_empty() {
                        continue;
                    }

                    let date = doc.query_selector_all("time[datetime]")
                        .iter()
                        .find(|&&t| doc.contains(wrapper, t))
                        .and_then(|&t| doc.get_attr(t, "datetime"))
                        .map(|s| s.split('T').next().unwrap_or("").to_string())
                        .unwrap_or_default();

                    let permalink = doc.query_selector_all("a.status__relative-time[href]")
                        .iter()
                        .find(|&&a| doc.contains(wrapper, a))
                        .and_then(|&a| doc.get_attr(a, "href"))
                        .map(|s| s.to_string())
                        .unwrap_or_default();

                    let author = if !display_name.is_empty() {
                        format!("{display_name} @{handle}")
                    } else {
                        format!("@{handle}")
                    };

                    replies.push(super::comments::CommentData {
                        author,
                        date,
                        content: reply_content,
                        depth: 0,
                        score: String::new(),
                        url: permalink,
                    });
                }

                let comments_html = if !replies.is_empty() {
                    super::comments::build_comment_tree(&replies)
                } else {
                    String::new()
                };

                super::comments::build_content_html("mastodon", &post_content, &comments_html)
            } else {
                String::new()
            };

            let og_title = doc.query_selector_all("meta[property=\"og:title\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .unwrap_or("")
                .to_string();
            let author = og_title.split(" (@").next().unwrap_or("").to_string();
            let site_name = doc.query_selector_all("meta[property=\"og:site_name\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .unwrap_or("mastodon.example")
                .to_string();
            let published = doc.query_selector_all("meta[property=\"og:published_time\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .map(|s| s.split('T').next().unwrap_or("").to_string())
                .unwrap_or_default();

            let fallback_site = site_name.clone();
            let site = if site_name.is_empty() { String::new() } else { site_name };

            ExtractorResult {
                content_html,
                content_selector: None,
                variables: BTreeMap::from([
                    ("title".into(), format!("Post by {author} on {fallback_site}")),
                    ("author".into(), author),
                    ("site".into(), site),
                    ("published".into(), published),
                ]),
            }
        }
    }
}

pub mod bbcode_data {
    use super::*;

    pub fn create(doc: &Document, _url: &str) -> Option<Box<dyn Extractor>> {
        let config = doc.query_selector_all("#application_config");
        if config.is_empty() {
            return None;
        }

        let config_id = config[0];
        let raw = doc.get_attr(config_id, "data-partnereventstore").unwrap_or("");
        if raw.is_empty() {
            return None;
        }

        let parsed: serde_json::Value = serde_json::from_str(raw).unwrap_or_default();
        let event = if parsed.is_array() {
            parsed.get(0).cloned().unwrap_or_default()
        } else {
            parsed
        };

        let body = event.get("announcement_body")
            .and_then(|ab| ab.get("body"))
            .and_then(|b| b.as_str())
            .unwrap_or("");
        if body.is_empty() {
            return None;
        }

        let headline = event.get("announcement_body")
            .and_then(|ab| ab.get("headline"))
            .and_then(|h| h.as_str())
            .unwrap_or("")
            .to_string();

        let title = if headline.is_empty() {
            event.get("event_name").and_then(|e| e.as_str()).unwrap_or("").to_string()
        } else {
            headline
        };

        let published = event.get("announcement_body")
            .and_then(|ab| ab.get("posttime"))
            .and_then(|pt| pt.as_i64())
            .map(|ts| {
                let secs = ts % 86400;
                let hours = secs / 3600;
                let mins = (secs % 3600) / 60;
                let secs_rem = secs % 60;
                format!("1970-01-01T{:02}:{:02}:{:02}.000Z", hours, mins, secs_rem)
            })
            .unwrap_or_default();

        let group_name = doc.get_attr(config_id, "data-groupvanityinfo")
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .and_then(|v| v.get("group_name").and_then(|g| g.as_str()).map(|s| s.to_string()))
            .unwrap_or_default();

        let content_html = bbcode_to_html(body);

        Some(Box::new(BbcodeExtractor {
            content_html,
            title,
            author: group_name,
            published,
        }))
    }

    struct BbcodeExtractor {
        content_html: String,
        title: String,
        author: String,
        published: String,
    }

    impl Extractor for BbcodeExtractor {
        fn can_extract(&self, _doc: &Document) -> bool {
            true
        }

        fn extract(&self, _doc: &Document) -> ExtractorResult {
            ExtractorResult {
                content_html: self.content_html.clone(),
                content_selector: None,
                variables: BTreeMap::from([
                    ("title".into(), self.title.clone()),
                    ("author".into(), self.author.clone()),
                    ("published".into(), self.published.clone()),
                    ("site".into(), String::new()),
                ]),
            }
        }
    }

    fn bbcode_to_html(bbcode: &str) -> String {
        let mut html = bbcode.to_string();

        html = regex::Regex::new(r"(?i)\[h1\]([\s\S]*?)\[/h1\]")
            .unwrap()
            .replace_all(&html, "<h1>$1</h1>")
            .to_string();
        html = regex::Regex::new(r"(?i)\[h2\]([\s\S]*?)\[/h2\]")
            .unwrap()
            .replace_all(&html, "<h2>$1</h2>")
            .to_string();
        html = regex::Regex::new(r"(?i)\[h3\]([\s\S]*?)\[/h3\]")
            .unwrap()
            .replace_all(&html, "<h3>$1</h3>")
            .to_string();

        html = regex::Regex::new(r"(?i)\[b\]([\s\S]*?)\[/b\]")
            .unwrap()
            .replace_all(&html, "<strong>$1</strong>")
            .to_string();
        html = regex::Regex::new(r"(?i)\[i\]([\s\S]*?)\[/i\]")
            .unwrap()
            .replace_all(&html, "<em>$1</em>")
            .to_string();
        html = regex::Regex::new(r"(?i)\[u\]([\s\S]*?)\[/u\]")
            .unwrap()
            .replace_all(&html, "<u>$1</u>")
            .to_string();
        html = regex::Regex::new(r"(?i)\[s\]([\s\S]*?)\[/s\]")
            .unwrap()
            .replace_all(&html, "<s>$1</s>")
            .to_string();

        // [url="..."] or [url=...]
        html = regex::Regex::new(r#"(?i)\[url=["']?([^"'\]]+)["']?\]([\s\S]*?)\[/url\]"#)
            .unwrap()
            .replace_all(&html, r#"<a href="$1">$2</a>"#)
            .to_string();

        html = regex::Regex::new(r"(?i)\[img\]([\s\S]*?)\[/img\]")
            .unwrap()
            .replace_all(&html, r#"<img src="$1">"#)
            .to_string();

        html = regex::Regex::new(r#"(?i)\[previewyoutube=["']?([^;'"]+)[^"'\]]*["']?\]\[\/previewyoutube\]"#)
            .unwrap()
            .replace_all(&html, r#"<img src="https://www.youtube.com/watch?v=$1">"#)
            .to_string();

        html = regex::Regex::new(r"(?i)\[list\]([\s\S]*?)\[/list\]")
            .unwrap()
            .replace_all(&html, |caps: &regex::Captures| {
                let inner = &caps[1];
                let items = regex::Regex::new(r"(?i)\[\*\]([\s\S]*?)(?=\[\*\]|\[/list\]|$)")
                    .unwrap()
                    .replace_all(inner, "<li>$1</li>");
                format!("<ul>{items}</ul>")
            })
            .to_string();

        html = regex::Regex::new(r"(?i)\[olist\]([\s\S]*?)\[/olist\]")
            .unwrap()
            .replace_all(&html, |caps: &regex::Captures| {
                let inner = &caps[1];
                let items = regex::Regex::new(r"(?i)\[\*\]([\s\S]*?)(?=\[\*\]|\[/olist\]|$)")
                    .unwrap()
                    .replace_all(inner, "<li>$1</li>");
                format!("<ol>{items}</ol>")
            })
            .to_string();

        html = regex::Regex::new(r"(?i)\[quote(?:=[^\]]+)?\]([\s\S]*?)\[/quote\]")
            .unwrap()
            .replace_all(&html, "<blockquote>$1</blockquote>")
            .to_string();

        html = regex::Regex::new(r"(?i)\[code\]([\s\S]*?)\[/code\]")
            .unwrap()
            .replace_all(&html, "<pre><code>$1</code></pre>")
            .to_string();

        html = regex::Regex::new(r"(?i)\[spoiler\]([\s\S]*?)\[/spoiler\]")
            .unwrap()
            .replace_all(&html, "<details><summary>Spoiler</summary>$1</details>")
            .to_string();

        html = regex::Regex::new(r"(?i)\[p\]([\s\S]*?)\[/p\]")
            .unwrap()
            .replace_all(&html, |caps: &regex::Captures| {
                let inner = caps[1].replace('\n', "<br>");
                format!("<p>{inner}</p>")
            })
            .to_string();

        html = html.replace('\n', "<br>");

        html = regex::Regex::new(r"\[[^\]]+\]")
            .unwrap()
            .replace_all(&html, "")
            .to_string();

        html
    }
}
