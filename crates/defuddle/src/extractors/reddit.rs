use crate::dom::Document;
use crate::extractors::{Extractor, ExtractorResult, comments};
use rustc_hash::FxHashMap;
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
            comments::build_comment_tree(&comments)
        } else {
            String::new()
        };

        let content_html = comments::build_content_html("reddit", &post_body, &comments_html);
        let _post_id = self
            .url
            .split("/comments/")
            .nth(1)
            .and_then(|s| s.split('/').next())
            .unwrap_or("")
            .to_string();

        ExtractorResult {
            content_html,
            variables: FxHashMap::from_iter([
                ("title".into(), post_title),
                ("author".into(), post_author),
                ("site".into(), format!("r/{subreddit}")),
                ("published".into(), published),
            ]),
        }
    }
}

fn collect_old_comments(doc: &Document) -> Vec<comments::CommentData> {
    let mut result = Vec::new();
    let commentarea = doc.query_selector_all(".commentarea .sitetable");
    if let Some(container) = commentarea.first().copied() {
        collect_comments_recursive(doc, container, 0, &mut result);
    }
    let mut seen = FxHashSet::default();
    result.retain(|comment| {
        let key = if comment.url.is_empty() {
            format!("{}\n{}\n{}", comment.author, comment.date, comment.content)
        } else {
            comment.url.clone()
        };
        seen.insert(key)
    });
    result
}

fn collect_comments_recursive(
    doc: &Document,
    container: crate::dom::NodeId,
    depth: u32,
    result: &mut Vec<comments::CommentData>,
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
        let date = crate::utils::extract_date(&time_el);
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

        result.push(comments::CommentData {
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
