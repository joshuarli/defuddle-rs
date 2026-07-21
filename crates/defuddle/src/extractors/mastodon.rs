use crate::dom::Document;
use crate::extractors::{comments, Extractor, ExtractorResult};
use rustc_hash::FxHashMap;

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
                    .map(|s| crate::utils::extract_date(s))
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

                replies.push(comments::CommentData {
                    author,
                    date,
                    content: reply_content,
                    depth: 0,
                    score: String::new(),
                    url: permalink,
                });
            }

            let comments_html = if !replies.is_empty() {
                comments::build_comment_tree(&replies)
            } else {
                String::new()
            };

            comments::build_content_html("mastodon", &post_content, &comments_html)
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
            .map(|s| crate::utils::extract_date(s))
            .unwrap_or_default();

        let fallback_site = site_name.clone();
        let site = if site_name.is_empty() {
            String::new()
        } else {
            site_name
        };

        ExtractorResult {
            content_html,
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
