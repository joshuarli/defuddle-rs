use rustc_hash::FxHashMap;

use crate::dom::Document;

pub struct ExtractorResult {
    pub content_html: String,
    #[allow(dead_code)]
    pub content_selector: Option<String>,
    pub variables: FxHashMap<String, String>,
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
            if (hostname.contains(domain.as_str())
                || url.contains(domain.as_str())
                || domain.is_empty())
                && let Some(extractor) = factory(doc, url)
                && extractor.can_extract(doc)
            {
                return Some(extractor);
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
    registry.register("x.com", twitter::create);
    registry.register("twitter.com", twitter::create);
    registry.register("", substack::create);

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
    use super::{Document, Extractor, ExtractorResult, FxHashMap};
    use rustc_hash::FxHashSet;

    pub fn create(doc: &Document, _url: &str) -> Option<Box<dyn Extractor>> {
        let main_post = doc.query_selector_all(".fatitem");
        let is_listing = !doc.query_selector_all("tr.athing").is_empty() && main_post.is_empty();

        if main_post.is_empty() && !is_listing {
            return None;
        }

        // Detect comment page: has .onstory inside .fatitem but no .titleline
        let is_comment_page = main_post.first().is_some_and(|&pid| {
            !doc.query_selector_all_in_subtree(pid, ".onstory")
                .is_empty()
                && doc
                    .query_selector_all_in_subtree(pid, ".titleline")
                    .is_empty()
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
                return self.extract_listing(doc);
            }
            self.extract_post(doc)
        }
    }

    impl HackerNewsExtractor {
        fn extract_listing(&self, doc: &Document) -> ExtractorResult {
            let stories: Vec<String> = doc
                .query_selector_all("tr.athing")
                .iter()
                .enumerate()
                .map(|(i, &athing)| {
                    let rank = i + 1;
                    let title_link = doc
                        .query_selector_all_in_subtree(athing, ".titleline a")
                        .first()
                        .and_then(|&id| doc.get_attr(id, "href").map(|s| s.to_string()))
                        .unwrap_or_default();

                    let title_text = doc
                        .query_selector_all_in_subtree(athing, ".titleline")
                        .first()
                        .map(|&id| doc.text_content(id).trim().to_string())
                        .unwrap_or_default();

                    let site_str = doc
                        .query_selector_all_in_subtree(athing, ".sitestr")
                        .first()
                        .map(|&id| doc.text_content(id).trim().to_string())
                        .unwrap_or_default();

                    let next_id = doc.next_element_sibling(athing);
                    let score = next_id
                        .and_then(|nid| doc.query_selector_all_in_subtree(nid, ".score").first().copied())
                        .map(|sid| doc.text_content(sid).trim().to_string())
                        .unwrap_or_default();

                    let author = next_id
                        .and_then(|nid| doc.query_selector_all_in_subtree(nid, ".hnuser").first().copied())
                        .map(|aid| doc.text_content(aid).trim().to_string())
                        .unwrap_or_default();

                    let item_id = doc
                        .get_attr(athing, "id")
                        .unwrap_or("")
                        .to_string();

                    let comments_info = next_id
                        .and_then(|nid| {
                            doc.query_selector_all_in_subtree(nid, "a")
                                .iter()
                                .find(|&&a| {
                                    doc.get_attr(a, "href")
                                        .map(|h| h.contains("item?id="))
                                        .unwrap_or(false)
                                })
                                .copied()
                        })
                        .map(|cid| doc.text_content(cid).trim().to_string());

                    let escaped_author = super::comments::escape_html(&author);
                    let escaped_title = super::comments::escape_html(&title_text);

                    let site_part = if !site_str.is_empty() {
                        format!(" ({site_str})")
                    } else {
                        String::new()
                    };

                    let comments_part = if let Some(ref ci) = comments_info {
                        let item_url = if !item_id.is_empty() {
                            format!("https://news.ycombinator.com/item?id={item_id}")
                        } else {
                            title_link.clone()
                        };
                        format!(" · [{ci}]({item_url})")
                    } else {
                        String::new()
                    };

                    format!(
                        "{rank}. [{escaped_title}]({title_link}){site_part}  \n\t{score} · by {escaped_author}{comments_part}"
                    )
                })
                .collect();

            let more_link = doc.query_selector_all(".morelink").first().and_then(|&id| {
                let href = doc
                    .get_attr(id, "href")
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                let text = doc.text_content(id).trim().to_string();
                if href.is_empty() {
                    None
                } else {
                    Some((href, text))
                }
            });

            let content_html = {
                let mut html = stories.join("\n");
                if let Some((href, text)) = more_link {
                    let escaped_text = super::comments::escape_html(&text);
                    let escaped_href = super::comments::escape_html(&href);
                    html.push_str(&format!("\n\n[{escaped_text}]({escaped_href})"));
                }
                html
            };

            let title = doc
                .query_selector_all("title")
                .first()
                .map(|id| {
                    let t = doc.text_content(*id);
                    t.trim()
                        .strip_suffix(" | Hacker News")
                        .unwrap_or(t.trim())
                        .trim()
                        .to_string()
                })
                .unwrap_or_else(|| "Hacker News".into());

            ExtractorResult {
                content_html,
                content_selector: None,
                variables: FxHashMap::from_iter([
                    ("title".into(), title),
                    ("site".into(), "Hacker News".into()),
                ]),
            }
        }

        fn extract_post(&self, doc: &Document) -> ExtractorResult {
            let post_content = self.get_post_content(doc);
            let comments = self.extract_comments(doc);
            let content_html =
                super::comments::build_content_html("hackernews", &post_content, &comments);

            let post_title = self.get_post_title(doc);
            let post_author = self.get_post_author(doc);
            let description = format!("{} - by {} on Hacker News", post_title, post_author);
            let published = self.get_post_date(doc);

            ExtractorResult {
                content_html,
                content_selector: None,
                variables: FxHashMap::from_iter([
                    ("title".into(), post_title),
                    ("author".into(), post_author),
                    ("site".into(), "Hacker News".into()),
                    ("description".into(), description),
                    ("published".into(), published),
                ]),
            }
        }

        fn get_post_title(&self, doc: &Document) -> String {
            if self.is_comment_page {
                let author = doc
                    .query_selector_all(".hnuser")
                    .first()
                    .map(|id| doc.text_content(*id).trim().to_string())
                    .unwrap_or_else(|| "[deleted]".into());
                let commtext = doc
                    .query_selector_all(".commtext")
                    .first()
                    .map(|id| doc.text_content(*id).trim().to_string())
                    .unwrap_or_default();
                let truncated: String = commtext.chars().take(50).collect();
                let preview = if commtext.chars().count() > 50 {
                    format!("{truncated}...")
                } else {
                    commtext
                };
                return format!("Comment by {author}: {preview}");
            }
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
                if let Some(_comment_id) = doc.query_selector_all("tr.athing").first().copied() {
                    let author = doc
                        .query_selector_all(".hnuser")
                        .first()
                        .map(|id| doc.text_content(*id).trim().to_string())
                        .unwrap_or_else(|| "[deleted]".into());
                    let commtext = doc
                        .query_selector_all(".commtext")
                        .first()
                        .map(|id| doc.inner_html(*id))
                        .unwrap_or_default();
                    let points = doc
                        .query_selector_all(".score")
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
            let titleline_url = doc
                .query_selector_all(".titleline a")
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
            let mut processed = FxHashSet::default();

            for comtr in comtrs {
                let id_attr = doc
                    .get_attr(comtr, "id")
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if id_attr.is_empty() || processed.contains(&id_attr) {
                    continue;
                }
                processed.insert(id_attr.clone());

                // Scope all queries to this comtr's subtree
                let indent_width = doc
                    .query_selector_all_in_subtree(comtr, ".ind img")
                    .first()
                    .copied()
                    .and_then(|id| doc.get_attr(id, "width"))
                    .and_then(|w| w.parse::<usize>().ok())
                    .unwrap_or(0);
                let depth = indent_width / 40;

                let commtext = doc
                    .query_selector_all_in_subtree(comtr, ".commtext")
                    .first()
                    .copied()
                    .map(|id| doc.inner_html(id))
                    .unwrap_or_default();
                if commtext.trim().is_empty() {
                    continue;
                }

                let author = doc
                    .query_selector_all_in_subtree(comtr, ".hnuser")
                    .first()
                    .copied()
                    .map(|id| doc.text_content(id).trim().to_string())
                    .unwrap_or_else(|| "[deleted]".into());

                let points = doc
                    .query_selector_all_in_subtree(comtr, ".score")
                    .first()
                    .copied()
                    .map(|id| doc.text_content(id).trim().to_string())
                    .unwrap_or_default();

                let age_title = doc
                    .query_selector_all_in_subtree(comtr, ".age")
                    .first()
                    .copied()
                    .and_then(|id| doc.get_attr(id, "title").map(|s| s.to_string()))
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
    use super::{Document, Extractor, ExtractorResult, FxHashMap};
    use rustc_hash::FxHashSet;

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
            let post_title = doc
                .query_selector_all("a.title")
                .first()
                .map(|id| doc.text_content(*id).trim().to_string())
                .unwrap_or_default();

            let post_author = doc
                .query_selector_all(".thing.link")
                .first()
                .and_then(|id| doc.get_attr(*id, "data-author"))
                .unwrap_or("")
                .to_string();
            let subreddit = doc
                .query_selector_all(".thing.link")
                .first()
                .and_then(|id| doc.get_attr(*id, "data-subreddit"))
                .unwrap_or("")
                .to_string();

            let published = doc
                .query_selector_all("time[datetime]")
                .first()
                .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string()))
                .unwrap_or_else(|| {
                    // Fallback: use date from first comment
                    doc.query_selector_all("time")
                        .first()
                        .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string()))
                        .unwrap_or_default()
                });

            let post_body = doc
                .query_selector_all(".usertext-body .md")
                .first()
                .map(|id| doc.inner_html(*id))
                .unwrap_or_default();

            let comments = collect_old_comments(doc);
            let comments_html = if !comments.is_empty() {
                super::comments::build_comment_tree(&comments)
            } else {
                String::new()
            };

            let content_html =
                super::comments::build_content_html("reddit", &post_body, &comments_html);
            let _post_id = self
                .url
                .split("/comments/")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .unwrap_or("")
                .to_string();

            ExtractorResult {
                content_html,
                content_selector: None,
                variables: FxHashMap::from_iter([
                    ("title".into(), post_title),
                    ("author".into(), post_author),
                    ("site".into(), format!("r/{subreddit}")),
                    ("published".into(), published),
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
        let thing_comments: Vec<_> = doc
            .children(container)
            .iter()
            .filter(|&c| {
                doc.is_element(*c)
                    && doc.tag_name(*c).eq_ignore_ascii_case("div")
                    && doc.get_attr(*c, "class").is_some_and(|cls| {
                        cls.split_whitespace()
                            .any(|w| w == "thing" || w == "comment")
                    })
            })
            .collect();
        let mut seen_children = FxHashSet::default();
        for comment in thing_comments {
            if !seen_children.insert(comment) {
                continue;
            }
            let author = doc
                .get_attr(*comment, "data-author")
                .unwrap_or("")
                .to_string();
            let permalink = doc
                .get_attr(*comment, "data-permalink")
                .unwrap_or("")
                .to_string();
            let score_text = doc
                .query_selector_all_in_subtree(*comment, ".entry .tagline .score.unvoted")
                .first()
                .map(|id| doc.text_content(*id).trim().to_string())
                .unwrap_or_default();
            let time_el = doc
                .query_selector_all_in_subtree(*comment, "time")
                .first()
                .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string()))
                .unwrap_or_default();
            let date = time_el.split('T').next().unwrap_or("").to_string();
            let body_html = doc
                .query_selector_all(".entry .usertext-body .md")
                .iter()
                .find(|&&el| doc.contains(*comment, el))
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

            let children: Vec<_> = doc
                .query_selector_all(".child > .sitetable")
                .into_iter()
                .filter(|&c| doc.contains(*comment, c))
                .collect();
            for child in children {
                collect_comments_recursive(doc, child, depth + 1, result);
            }
        }
    }
}

pub mod github {
    use super::{Document, Extractor, ExtractorResult, FxHashMap};

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
        let has_indicator = indicators
            .iter()
            .any(|sel| !doc.query_selector_all(sel).is_empty());

        if !has_indicator {
            return None;
        }

        let repo_info = extract_repo_info(url);
        let number = extract_number(url);

        Some(Box::new(GitHubExtractor {
            repo_info,
            number,
            is_pr,
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
        #[allow(dead_code)]
        number: String,
        is_pr: bool,
    }

    impl Extractor for GitHubExtractor {
        fn can_extract(&self, _doc: &Document) -> bool {
            true
        }

        fn extract(&self, doc: &Document) -> ExtractorResult {
            let (owner, repo) = &self.repo_info;

            let title = doc
                .query_selector_all("title")
                .first()
                .map(|id| doc.text_content(*id).trim().to_string())
                .unwrap_or_default();

            let (post_content, author, published) = if self.is_pr {
                let body = doc
                    .query_selector_all(".pull-discussion-timeline .js-discussion > .timeline-comment-group:first-of-type .comment-body.markdown-body")
                    .first()
                    .map(|id| doc.inner_html(*id))
                    .unwrap_or_default();
                let author_val = doc
                    .query_selector_all(".pull-discussion-timeline .js-discussion > .timeline-comment-group:first-of-type .author")
                    .first()
                    .and_then(|id| doc.get_attr(*id, "href").map(|s| s.to_string()))
                    .map(|href| href.trim_start_matches('/').to_string())
                    .unwrap_or_default();
                let date = doc
                    .query_selector_all(".pull-discussion-timeline .js-discussion > .timeline-comment-group:first-of-type relative-time")
                    .first()
                    .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string()))
                    .unwrap_or_default();
                (body, author_val, date)
            } else {
                let body = doc
                    .query_selector_all(".js-comment-body, .comment-body.markdown-body")
                    .first()
                    .map(|id| doc.inner_html(*id))
                    .unwrap_or_default();
                let author_val = doc
                    .query_selector_all(".author.Link--primary, .timeline-comment-header .author")
                    .first()
                    .map(|id| doc.text_content(*id).trim().to_string())
                    .unwrap_or_default();
                let date = doc
                    .query_selector_all("relative-time")
                    .first()
                    .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string()))
                    .unwrap_or_default();
                (body, author_val, date)
            };

            let comments = extract_github_comments(doc);
            let comments_html = if !comments.is_empty() {
                super::comments::build_comment_tree(&comments)
            } else {
                String::new()
            };

            let content_html =
                super::comments::build_content_html("github", &post_content, &comments_html);

            ExtractorResult {
                content_html,
                content_selector: None,
                variables: FxHashMap::from_iter([
                    ("title".into(), title),
                    ("site".into(), format!("GitHub - {owner}/{repo}")),
                    ("author".into(), author),
                    ("published".into(), published),
                    ("description".into(), String::new()),
                ]),
            }
        }
    }

    fn extract_github_comments(doc: &Document) -> Vec<super::comments::CommentData> {
        let mut comment_data = Vec::new();

        for header_id in doc.query_selector_all(".js-comment-container .timeline-comment-header, .TimelineItem.js-comment-container .timeline-comment-header") {
            let container = match doc.closest(header_id, ".js-comment-container,.TimelineItem.js-comment-container,.timeline-comment") {
                Some(c) => c, None => continue,
            };
            let author = doc.query_selector_all_in_subtree(header_id, ".author").first()
                .and_then(|id| doc.get_attr(*id, "href"))
                .map(|href| href.trim_start_matches('/').to_string()).unwrap_or_default();
            if author.is_empty() { continue; }
            let datetime = doc.query_selector_all_in_subtree(header_id, "relative-time").first()
                .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string())).unwrap_or_default();
            let date = datetime.split('T').next().unwrap_or("").to_string();
            let body = doc.query_selector_all_in_subtree(container, ".comment-body.markdown-body").first()
                .map(|id| doc.inner_html(*id)).unwrap_or_default();
            if body.trim().is_empty() { continue; }
            comment_data.push(super::comments::CommentData {
                author, date, content: body, depth: 0, score: String::new(), url: String::new(),
            });
        }

        for header_id in
            doc.query_selector_all(".js-inline-comments-container .review-comment .ml-n1.clearfix")
        {
            let container = match doc.closest(header_id, ".review-comment,.timeline-comment-group")
            {
                Some(c) => c,
                None => continue,
            };
            let author = doc
                .query_selector_all_in_subtree(header_id, ".author")
                .first()
                .and_then(|id| doc.get_attr(*id, "href"))
                .map(|href| href.trim_start_matches('/').to_string())
                .unwrap_or_default();
            if author.is_empty() {
                continue;
            }
            let datetime = doc
                .query_selector_all_in_subtree(header_id, "relative-time")
                .first()
                .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string()))
                .unwrap_or_default();
            let date = datetime.split('T').next().unwrap_or("").to_string();
            let body = doc
                .query_selector_all_in_subtree(container, ".comment-body.markdown-body")
                .first()
                .map(|id| doc.inner_html(*id))
                .unwrap_or_default();
            if body.trim().is_empty() {
                continue;
            }
            comment_data.push(super::comments::CommentData {
                author,
                date,
                content: body,
                depth: 0,
                score: String::new(),
                url: String::new(),
            });
        }

        comment_data
    }
}

pub mod chatgpt {
    use super::{Document, Extractor, ExtractorResult, FxHashMap};

    pub fn create(doc: &Document, _url: &str) -> Option<Box<dyn Extractor>> {
        if doc
            .query_selector_all("[data-testid^=\"conversation-turn-\"]")
            .is_empty()
        {
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
                            let content_els = doc.query_selector_all_in_subtree(
                                el,
                                ".markdown, .whitespace-pre-wrap",
                            );
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
                message_content = regex_lite::Regex::new(r"<p[^>]*>\s*</p>")
                    .unwrap()
                    .replace_all(&message_content, "")
                    .to_string();

                let trimmed = message_content.trim().to_string();
                if !trimmed.is_empty() {
                    messages.push((heading, trimmed));
                }
            }

            let html = build_conversation_html(&messages, &footnotes);

            ExtractorResult {
                content_html: html,
                content_selector: None,
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
        let re = regex_lite::Regex::new(
            r#"<span[^>]*data-testid="webpage-citation-pill"[^>]*>[\s\S]*?<a\b[^>]*href="([^"]*)"[^>]*>[\s\S]*?</a>[\s\S]*?</span>"#,
        )
        .unwrap();
        re.replace_all(html, |caps: &regex_lite::Captures| {
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
            let safe = super::comments::escape_html(author);
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
}

pub mod substack {
    use super::{Document, Extractor, ExtractorResult, FxHashMap};

    pub fn create(doc: &Document, _url: &str) -> Option<Box<dyn Extractor>> {
        if !doc.query_selector_all("div.body.markup").is_empty() {
            return Some(Box::new(SubstackExtractor { mode: 0 }));
        }
        let scripts = doc.query_selector_all("script");
        for sc in scripts {
            let text = doc.text_content(sc);
            if text.contains("window._preloads") && text.contains("body_html") {
                return Some(Box::new(SubstackExtractor { mode: 2 }));
            }
        }
        if !doc
            .query_selector_all(".ProseMirror, .FeedProseMirror")
            .is_empty()
        {
            return Some(Box::new(SubstackExtractor { mode: 1 }));
        }
        None
    }

    struct SubstackExtractor {
        mode: u8,
    }

    impl Extractor for SubstackExtractor {
        fn can_extract(&self, _doc: &Document) -> bool {
            true
        }

        fn extract(&self, doc: &Document) -> ExtractorResult {
            let meta_title = doc
                .query_selector_all("meta[property=\"og:title\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .unwrap_or("")
                .to_string();
            let meta_site = doc
                .query_selector_all("meta[property=\"og:site_name\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .unwrap_or("")
                .to_string();
            let meta_author = doc
                .query_selector_all("meta[name=\"author\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .unwrap_or("")
                .to_string();
            let meta_published = doc
                .query_selector_all("meta[property=\"article:published_time\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .unwrap_or("")
                .to_string();

            let (content_html, title, author, site) = match self.mode {
                0 => {
                    let body_html = substack_article_body(doc);
                    let t = if meta_title.is_empty() {
                        doc.query_selector_all("title")
                            .first()
                            .map(|id| doc.text_content(*id).trim().to_string())
                            .unwrap_or_default()
                    } else {
                        meta_title.clone()
                    };
                    let a = if meta_author.is_empty() {
                        doc.query_selector_all("a[href*=\"substack.com/@\"]")
                            .first()
                            .map(|id| doc.text_content(*id).trim().to_string())
                            .unwrap_or_default()
                    } else {
                        meta_author.clone()
                    };
                    let s = if meta_site.is_empty() {
                        "Substack".to_string()
                    } else {
                        meta_site.clone()
                    };
                    (body_html, t, a, s)
                }
                1 => {
                    let feed_perm = doc.query_selector_all("[class*=\"feedPermalinkUnit\"]");
                    let permalink = feed_perm.first();
                    let note_html = permalink
                        .and_then(|pid| {
                            doc.query_selector_all_in_subtree(
                                *pid,
                                "div.ProseMirror.FeedProseMirror",
                            )
                            .first()
                            .copied()
                        })
                        .or_else(|| {
                            doc.query_selector_all("div.ProseMirror.FeedProseMirror")
                                .first()
                                .copied()
                        })
                        .map(|id| doc.inner_html(id))
                        .unwrap_or_default();
                    let a = meta_title
                        .rsplit(" (@")
                        .last()
                        .unwrap_or(&meta_title)
                        .trim()
                        .to_string();
                    (note_html, meta_title, a, "Substack".to_string())
                }
                _ => substack_preload(doc),
            };

            ExtractorResult {
                content_html,
                content_selector: None,
                variables: FxHashMap::from_iter([
                    ("title".into(), title),
                    ("author".into(), author),
                    ("site".into(), site),
                    ("published".into(), meta_published),
                ]),
            }
        }
    }

    fn substack_article_body(doc: &Document) -> String {
        let all_body = doc.query_selector_all("div.body.markup");
        let body_id = match all_body.first() {
            Some(id) => *id,
            None => return String::new(),
        };
        let mut html = doc.inner_html(body_id);
        html = regex_lite::Regex::new(r#"<p[^>]*class="button-wrapper"[^>]*>[\s\S]*?</p>"#)
            .unwrap()
            .replace_all(&html, "")
            .to_string();
        html = regex_lite::Regex::new(
            r#"<div[^>]*data-component-name="SubscribeWidgetToDOM"[^>]*>[\s\S]*?</div>"#,
        )
        .unwrap()
        .replace_all(&html, "")
        .to_string();
        html = regex_lite::Regex::new(
            r#"<div[^>]*data-component-name="DigestPostEmbed"[^>]*>[\s\S]*?</div>"#,
        )
        .unwrap()
        .replace_all(&html, "")
        .to_string();
        html = regex_lite::Regex::new(r#"<div><hr></div>\s*<p>For More[\s\S]*?</p>\s*"#)
            .unwrap()
            .replace(&html, "")
            .to_string();
        html.trim().to_string()
    }

    fn substack_preload(doc: &Document) -> (String, String, String, String) {
        let scripts = doc.query_selector_all("script");
        for sc in scripts {
            let text = doc.text_content(sc);
            if !text.contains("window._preloads") || !text.contains("body_html") {
                continue;
            }
            if let Some(start) = text.find("JSON.parse(\"") {
                let start = start + "JSON.parse(\"".len();
                let mut i = start;
                let chars: Vec<char> = text.chars().collect();
                while i < chars.len() {
                    if chars[i] == '\\' {
                        i += 2;
                    } else if chars[i] == '"' {
                        break;
                    } else {
                        i += 1;
                    }
                }
                let inner: String = chars[start..i].iter().collect();
                if let Ok(json_str) = miniserde::json::from_str::<String>(&format!("\"{}\"", inner))
                    && let Ok(data) = miniserde::json::from_str::<miniserde::json::Value>(&json_str)
                    && let Some(post) = crate::schema::object_value(&data, "feedData")
                        .and_then(|f| crate::schema::object_value(f, "initialPost"))
                        .and_then(|p| crate::schema::object_value(p, "post"))
                {
                    let title = crate::schema::object_value(post, "title")
                        .and_then(crate::schema::as_str)
                        .unwrap_or("")
                        .to_string();
                    let author = crate::schema::object_value(post, "publishedBylines")
                        .and_then(crate::schema::as_array)
                        .and_then(|arr| arr.first())
                        .and_then(|b| crate::schema::object_value(b, "name"))
                        .and_then(crate::schema::as_str)
                        .unwrap_or("")
                        .to_string();
                    let body = crate::schema::object_value(post, "body_html")
                        .and_then(crate::schema::as_str)
                        .unwrap_or("")
                        .to_string();
                    if !body.is_empty() {
                        return (body, title, author, "Substack".to_string());
                    }
                }
                break;
            }
        }
        (
            String::new(),
            String::new(),
            String::new(),
            "Substack".to_string(),
        )
    }
}

pub mod mastodon {
    use super::{Document, Extractor, ExtractorResult, FxHashMap};

    pub fn create(doc: &Document, _url: &str) -> Option<Box<dyn Extractor>> {
        if doc
            .query_selector_all(".detailed-status__wrapper, .status__wrapper")
            .is_empty()
        {
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
            let main_post = doc
                .query_selector_all(".detailed-status__wrapper")
                .first()
                .copied();

            let main_handle = main_post
                .and_then(|mp| {
                    doc.query_selector_all(".display-name__account")
                        .iter()
                        .find(|&&h| doc.contains(mp, h))
                        .map(|&h| {
                            doc.text_content(h)
                                .trim()
                                .trim_start_matches('@')
                                .to_string()
                        })
                })
                .unwrap_or_default();

            let content_html = if let Some(main_id) = main_post {
                let text = doc
                    .query_selector_all(".status__content__text")
                    .iter()
                    .find(|&&s| doc.contains(main_id, s))
                    .map(|&s| doc.inner_html(s).trim().to_string())
                    .unwrap_or_default();

                let mut post_parts: Vec<String> = Vec::new();
                if !text.is_empty() {
                    post_parts.push(text);
                }

                // Classify reply wrappers: self-replies form thread, others form comments
                let reply_wrappers: Vec<_> = doc
                    .query_selector_all(".status__wrapper")
                    .into_iter()
                    .filter(|&w| {
                        w != main_id
                            && doc
                                .query_selector_all(".status[data-id]")
                                .iter()
                                .any(|&s| doc.contains(w, s))
                    })
                    .collect();

                let mut thread_ended = false;
                let mut comment_wrappers = Vec::new();

                for &wrapper in &reply_wrappers {
                    let wrapper_handle = doc
                        .query_selector_all(".display-name__account")
                        .iter()
                        .find(|&&h| doc.contains(wrapper, h))
                        .map(|&h| {
                            doc.text_content(h)
                                .trim()
                                .trim_start_matches('@')
                                .to_string()
                        })
                        .unwrap_or_default();

                    if !thread_ended && wrapper_handle == main_handle {
                        let thread_text = doc
                            .query_selector_all(".status__content__text")
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
                    let handle = doc
                        .query_selector_all(".display-name__account")
                        .iter()
                        .find(|&&h| doc.contains(wrapper, h))
                        .map(|&h| {
                            doc.text_content(h)
                                .trim()
                                .trim_start_matches('@')
                                .to_string()
                        })
                        .unwrap_or_default();

                    let display_name = doc
                        .query_selector_all(".display-name__html")
                        .iter()
                        .find(|&&dn| doc.contains(wrapper, dn))
                        .map(|&dn| doc.text_content(dn).trim().to_string())
                        .unwrap_or_default();

                    let reply_content = doc
                        .query_selector_all(".status__content__text")
                        .iter()
                        .find(|&&rc| doc.contains(wrapper, rc))
                        .map(|&rc| doc.inner_html(rc).trim().to_string())
                        .unwrap_or_default();

                    if reply_content.is_empty() {
                        continue;
                    }

                    let date = doc
                        .query_selector_all("time[datetime]")
                        .iter()
                        .find(|&&t| doc.contains(wrapper, t))
                        .and_then(|&t| doc.get_attr(t, "datetime"))
                        .map(|s| s.split('T').next().unwrap_or("").to_string())
                        .unwrap_or_default();

                    let permalink = doc
                        .query_selector_all("a.status__relative-time[href]")
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

            let og_title = doc
                .query_selector_all("meta[property=\"og:title\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .unwrap_or("")
                .to_string();
            let author = og_title.split(" (@").next().unwrap_or("").to_string();
            let site_name = doc
                .query_selector_all("meta[property=\"og:site_name\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .unwrap_or("mastodon.example")
                .to_string();
            let published = doc
                .query_selector_all("meta[property=\"og:published_time\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .map(|s| s.split('T').next().unwrap_or("").to_string())
                .unwrap_or_default();

            let fallback_site = site_name.clone();
            let site = if site_name.is_empty() {
                String::new()
            } else {
                site_name
            };

            ExtractorResult {
                content_html,
                content_selector: None,
                variables: FxHashMap::from_iter([
                    (
                        "title".into(),
                        format!("Post by {author} on {fallback_site}"),
                    ),
                    ("author".into(), author),
                    ("site".into(), site),
                    ("published".into(), published),
                ]),
            }
        }
    }
}

pub mod twitter {
    use super::{Document, Extractor, ExtractorResult, FxHashMap};

    pub fn create(doc: &Document, url: &str) -> Option<Box<dyn Extractor>> {
        // Check for X Article pages (twitterArticleRichTextView with Draft.js blocks)
        if !doc
            .query_selector_all("[data-testid=\"twitterArticleRichTextView\"]")
            .is_empty()
        {
            return Some(Box::new(XExtractor {
                mode: XMode::Article,
                url: url.to_string(),
            }));
        }

        // Check for conversation timeline pages (localized)
        let sections = doc.query_selector_all("section[aria-label]");
        for section_id in &sections {
            if let Some(label) = doc.get_attr(*section_id, "aria-label") {
                let lower = label.to_lowercase();
                if (lower.contains("time")
                    || lower.contains("conversation")
                    || lower.contains("对话"))
                    && !doc
                        .query_selector_all_in_subtree(
                            *section_id,
                            "article[data-testid=\"tweet\"]",
                        )
                        .is_empty()
                {
                    return Some(Box::new(XExtractor {
                        mode: XMode::Conversation,
                        url: url.to_string(),
                    }));
                }
            }
        }

        // Check for tweet articles on x.com/twitter.com pages
        if !doc
            .query_selector_all("article[data-testid=\"tweet\"]")
            .is_empty()
        {
            let meta_site = doc
                .query_selector_all("meta[property=\"og:site_name\"]")
                .first()
                .and_then(|id| doc.get_attr(*id, "content"))
                .unwrap_or("");
            if meta_site.contains("X") || meta_site.contains("Twitter") {
                return Some(Box::new(XExtractor {
                    mode: XMode::Conversation,
                    url: url.to_string(),
                }));
            }
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

    impl Extractor for XExtractor {
        fn can_extract(&self, _doc: &Document) -> bool {
            true
        }

        fn extract(&self, doc: &Document) -> ExtractorResult {
            match self.mode {
                XMode::Article => extract_article(doc, &self.url),
                XMode::Conversation => extract_conversation(doc),
            }
        }
    }

    /// Extract content from an X Article page (twitterArticleRichTextView with Draft.js blocks).
    fn extract_article(doc: &Document, url: &str) -> ExtractorResult {
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
            .unwrap_or("")
            .to_string();

        // Build content HTML
        let mut content_html = String::new();
        if has_title_block && !article_title.is_empty() {
            let safe = super::comments::escape_html(&article_title);
            content_html.push_str(&format!("<div>{safe}</div>\n<p>\u{00B7}</p>\n"));
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

        ExtractorResult {
            content_html: content_html.trim().to_string(),
            content_selector: None,
            variables: FxHashMap::from_iter([
                ("title".into(), title),
                ("author".into(), author),
                ("site".into(), "X (Twitter)".into()),
                ("published".into(), published),
            ]),
        }
    }

    /// Extract a conversation timeline (localized section with tweets).
    fn extract_conversation(doc: &Document) -> ExtractorResult {
        let sections = doc.query_selector_all("section[aria-label]");
        let mut conversation_section: Option<crate::NodeId> = None;

        for &section_id in &sections {
            if let Some(label) = doc.get_attr(section_id, "aria-label") {
                let lower = label.to_lowercase();
                if (lower.contains("time")
                    || lower.contains("conversation")
                    || lower.contains("对话"))
                    && !doc
                        .query_selector_all_in_subtree(section_id, "article[data-testid=\"tweet\"]")
                        .is_empty()
                {
                    conversation_section = Some(section_id);
                    break;
                }
            }
        }

        let tweets = if let Some(sec) = conversation_section {
            doc.query_selector_all_in_subtree(sec, "article[data-testid=\"tweet\"]")
        } else {
            doc.query_selector_all("article[data-testid=\"tweet\"]")
        };

        let mut main_post = String::new();
        let mut replies: Vec<super::comments::CommentData> = Vec::new();
        let mut author_handle = String::new();

        for (i, &tweet_id) in tweets.iter().enumerate() {
            let user_name = doc
                .query_selector_all_in_subtree(tweet_id, "[data-testid=\"User-Name\"]")
                .first()
                .map(|uid| {
                    let links = doc.query_selector_all_in_subtree(*uid, "a");
                    let display_name = links
                        .first()
                        .map(|&id| doc.text_content(id).trim().to_string())
                        .unwrap_or_default();
                    let handle = links
                        .get(1)
                        .map(|&id| doc.text_content(id).trim().to_string())
                        .unwrap_or_default();
                    if display_name.is_empty() && handle.is_empty() {
                        String::new()
                    } else if display_name.is_empty() {
                        handle.clone()
                    } else if handle.is_empty() {
                        display_name.clone()
                    } else {
                        format!("{display_name} {handle}")
                    }
                })
                .unwrap_or_default();

            let date = doc
                .query_selector_all_in_subtree(tweet_id, "time[datetime]")
                .first()
                .and_then(|&tid| doc.get_attr(tid, "datetime"))
                .unwrap_or("")
                .to_string();

            let permalink = doc
                .query_selector_all_in_subtree(tweet_id, "time[datetime]")
                .first()
                .and_then(|tid| {
                    let time_parent = doc.parent(*tid)?;
                    if doc.tag_name(time_parent) == "a" {
                        doc.get_attr(time_parent, "href").map(|s| {
                            if s.starts_with('/') {
                                format!("https://x.com{s}")
                            } else {
                                s.to_string()
                            }
                        })
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let content = doc
                .query_selector_all_in_subtree(tweet_id, "[data-testid=\"tweetText\"]")
                .first()
                .map(|&tt_id| doc.text_content(tt_id).trim().to_string())
                .unwrap_or_default();

            if i == 0 {
                main_post = content;
                author_handle = doc
                    .query_selector_all_in_subtree(tweet_id, "[data-testid=\"User-Name\"]")
                    .first()
                    .and_then(|uid| {
                        let links = doc.query_selector_all_in_subtree(*uid, "a");
                        links
                            .get(1)
                            .map(|&id| doc.text_content(id).trim().to_string())
                    })
                    .unwrap_or_default();
            } else {
                replies.push(super::comments::CommentData {
                    author: user_name,
                    date,
                    content,
                    depth: 0,
                    score: String::new(),
                    url: permalink,
                });
            }
        }

        let comments_html = if !replies.is_empty() {
            format!(
                "\n<hr>\n## Comments\n\n{}",
                super::comments::build_comment_tree(&replies)
            )
        } else {
            String::new()
        };

        let published = tweets
            .first()
            .and_then(|&tid| {
                doc.query_selector_all_in_subtree(tid, "time[datetime]")
                    .first()
                    .and_then(|&time_id| doc.get_attr(time_id, "datetime"))
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();

        let content_html = format!("{main_post}{comments_html}");
        let fallback_title = if author_handle.is_empty() {
            "Post on X".to_string()
        } else {
            format!("Post by {author_handle} on X")
        };

        ExtractorResult {
            content_html: content_html.trim().to_string(),
            content_selector: None,
            variables: FxHashMap::from_iter([
                ("title".into(), fallback_title),
                ("author".into(), author_handle),
                ("site".into(), "X".into()),
                ("published".into(), published),
            ]),
        }
    }

    /// Recursively walk the DOM tree to find Draft.js blocks in document order.
    fn find_and_process_blocks(doc: &Document, node: crate::NodeId, html: &mut String) {
        let class = doc.get_attr(node, "class").unwrap_or("");
        let testid = doc.get_attr(node, "data-testid").unwrap_or("");

        let is_block = class.contains("longform-unstyled")
            || class.contains("longform-header-two")
            || class.contains("longform-header-three")
            || class.contains("longform-header-four")
            || testid == "markdown-code-block"
            || testid == "tweetPhoto"
            || class.contains("public-DraftStyleDefault-ol")
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
        } else if testid == "markdown-code-block" {
            let code_html = extract_code_block_html(doc, block_id);
            if !code_html.is_empty() {
                html.push_str(&format!("{code_html}\n"));
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
        let blocks =
            doc.query_selector_all_in_subtree(block_id, "div.public-DraftStyleDefault-block");
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
        let blocks =
            doc.query_selector_all_in_subtree(node_id, "div.public-DraftStyleDefault-block");
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
                        result.push_str(&super::comments::escape_html(&text));
                    }
                } else {
                    for &child_id in doc.children(node_id) {
                        collect_formatted_text(doc, child_id, result);
                    }
                }
            }
        }
    }

    fn extract_code_block_html(doc: &Document, block_id: crate::NodeId) -> String {
        let code = doc
            .query_selector_all_in_subtree(block_id, "code")
            .first()
            .copied();
        let pre = doc
            .query_selector_all_in_subtree(block_id, "pre")
            .first()
            .copied();
        if let Some(code_id) = code {
            let lang = doc
                .get_attr(code_id, "class")
                .unwrap_or("")
                .split_whitespace()
                .find(|c| c.starts_with("language-"))
                .map(|c| &c["language-".len()..])
                .unwrap_or("");
            let code_text = if let Some(pre_val) = pre {
                doc.text_content(pre_val)
            } else {
                doc.text_content(code_id)
            };
            let escaped = super::comments::escape_html(code_text.trim());
            if lang.is_empty() {
                format!("<pre><code>{escaped}</code></pre>")
            } else {
                format!("<pre><code class=\"language-{lang}\">{escaped}</code></pre>")
            }
        } else if let Some(pre_id) = pre {
            let code_text = doc.text_content(pre_id);
            let escaped = super::comments::escape_html(code_text.trim());
            format!("<pre><code>{escaped}</code></pre>")
        } else {
            String::new()
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
}
