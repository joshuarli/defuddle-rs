use crate::dom::Document;
use crate::extractors::{comments, Extractor, ExtractorResult};
use rustc_hash::FxHashMap;
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
            .map(|&athing| {
                let title_link = doc
                    .query_selector_all_in_subtree(athing, ".titleline a")
                    .first()
                    .and_then(|&id| doc.get_attr(id, "href").map(|s| s.to_string()))
                    .unwrap_or_default();
                let title_link = if title_link.starts_with("http://") || title_link.starts_with("https://") {
                    title_link
                } else {
                    format!("https://news.ycombinator.com/{}", title_link.trim_start_matches('/'))
                };

                let title_text = doc
                    .query_selector_all_in_subtree(athing, ".titleline a")
                    .first()
                    .map(|&id| doc.text_content(id).trim().to_string())
                    .unwrap_or_default();

                let site_str = doc
                    .query_selector_all_in_subtree(athing, ".sitestr")
                    .first()
                    .map(|&id| doc.text_content(id).trim().to_string())
                    .unwrap_or_default();

                let mut next_id = doc.next_element_sibling(athing);
                for _ in 0..3 {
                    if next_id
                        .is_some_and(|id| !doc.query_selector_all_in_subtree(id, ".subtext").is_empty())
                    {
                        break;
                    }
                    next_id = next_id.and_then(|id| doc.next_element_sibling(id));
                }
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
                                doc.text_content(a).contains("comments")
                            })
                            .copied()
                    })
                    .map(|cid| doc.text_content(cid).trim().to_string());

                let escaped_author = comments::escape_html(&author);
                let escaped_title = comments::escape_html(&title_text);

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
                    let ci = ci.replace('\u{a0}', " ");
                    format!(
                        " · <a href=\"{}\">{}</a>",
                        comments::escape_html(&item_url),
                        comments::escape_html(&ci)
                    )
                } else {
                    String::new()
                };

                format!(
                    "<li><a href=\"{}\">{escaped_title}</a>{site_part}<div>{score} · by {escaped_author}{comments_part}</div></li>",
                    comments::escape_html(&title_link)
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
            let mut html = format!("<ol>{}</ol>", stories.join("\n"));
            if let Some((href, text)) = more_link {
                let escaped_text = comments::escape_html(&text);
                let absolute_href =
                    if href.starts_with("http://") || href.starts_with("https://") {
                        href
                    } else {
                        format!(
                            "https://news.ycombinator.com/{}",
                            href.trim_start_matches('/')
                        )
                    };
                let escaped_href = comments::escape_html(&absolute_href);
                html.push_str(&format!(
                    "\n\n<a href=\"{escaped_href}\">{escaped_text}</a>"
                ));
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
            comments::build_content_html("hackernews", &post_content, &comments);

        let post_title = self.get_post_title(doc);
        let post_author = self.get_post_author(doc);
        let description = format!("{} - by {} on Hacker News", post_title, post_author);
        let published = self.get_post_date(doc);

        ExtractorResult {
            content_html,
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
                return comments::build_comment(&comments::CommentData {
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
                comments::escape_html(&titleline_url),
                comments::escape_html(&titleline_url)
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

            comment_data.push(comments::CommentData {
                author,
                date,
                content: commtext,
                depth: depth as u32,
                score: points,
                url: comment_url,
            });
        }

        comments::build_comment_tree(&comment_data)
    }
}
