use crate::dom::Document;
use crate::extractors::{HtmlExtraction, SiteAdapter, SiteExtraction, comments};
use crate::pipeline::ExtractionContext;
use rustc_hash::FxHashMap;

pub(crate) fn adapter() -> Box<dyn SiteAdapter> {
    Box::new(MastodonAdapter)
}

fn full_handle(doc: &Document, container: crate::NodeId) -> String {
    doc.query_selector_all(".display-name__account")
        .iter()
        .find(|&&h| doc.contains(container, h))
        .map(|&h| {
            doc.text_content(h)
                .trim()
                .trim_start_matches('@')
                .to_string()
        })
        .unwrap_or_default()
}

fn short_handle(full_handle: &str) -> String {
    full_handle.split('@').next().unwrap_or("").to_string()
}

fn absolute_url(page_url: &str, href: &str) -> String {
    if href.starts_with("http") {
        return href.to_string();
    }
    let origin = page_url
        .find("://")
        .and_then(|scheme_end| {
            page_url[scheme_end + 3..]
                .find('/')
                .map(|path_start| &page_url[..scheme_end + 3 + path_start])
        })
        .unwrap_or(page_url);
    format!("{origin}{href}")
}

struct MastodonAdapter;

impl SiteAdapter for MastodonAdapter {
    fn matches(&self, context: &ExtractionContext<'_>) -> bool {
        !context
            .doc
            .query_selector_all(".detailed-status__wrapper, .status__wrapper")
            .is_empty()
    }

    fn extract(&self, context: &mut ExtractionContext<'_>) -> SiteExtraction {
        let mastodon_extraction = MastodonExtractor.extract(&context.doc, context.page_url);
        crate::extractors::site_extraction_from_html(
            mastodon_extraction.content_html,
            mastodon_extraction.variables,
        )
    }
}

struct MastodonExtractor;

impl MastodonExtractor {
    fn post_content(doc: &Document, container: crate::NodeId) -> String {
        let mut parts = Vec::new();
        let text = Self::post_text(doc, container);
        if !text.is_empty() {
            parts.push(text);
        }
        let images = Self::post_images(doc, container);
        if !images.is_empty() {
            parts.push(images);
        }
        let card = Self::post_card(doc, container);
        if !card.is_empty() {
            parts.push(card);
        }
        parts.join("\n")
    }

    fn post_text(doc: &Document, container: crate::NodeId) -> String {
        let Some(text_id) = doc
            .query_selector_all(".status__content__text")
            .into_iter()
            .find(|&s| doc.contains(container, s))
        else {
            return String::new();
        };
        let (mut clone, root) = Document::clone_subtree(doc, text_id);
        let invisible = clone.query_selector_all_in_subtree(root, "span.invisible");
        for id in invisible {
            clone.remove(id);
        }
        clone.inner_html(root).trim().to_string()
    }

    fn post_images(doc: &Document, container: crate::NodeId) -> String {
        let Some(gallery) = doc
            .query_selector_all(".media-gallery")
            .into_iter()
            .find(|&g| doc.contains(container, g))
        else {
            return String::new();
        };
        doc.query_selector_all_in_subtree(gallery, ".media-gallery__item-thumbnail")
            .into_iter()
            .filter_map(|link| {
                let href = doc.get_attr(link, "href")?;
                if href.is_empty() {
                    return None;
                }
                let alt = doc
                    .query_selector_all_in_subtree(link, "img")
                    .first()
                    .and_then(|id| doc.get_attr(*id, "alt"))
                    .unwrap_or("");
                Some(format!(
                    "<img src=\"{}\" alt=\"{}\" />",
                    crate::content_utils::escape_html(href),
                    crate::content_utils::escape_html(alt)
                ))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn post_card(doc: &Document, container: crate::NodeId) -> String {
        let Some(card) = doc
            .query_selector_all("a.status-card[href]")
            .into_iter()
            .find(|&a| doc.contains(container, a))
        else {
            return String::new();
        };
        let href = doc.get_attr(card, "href").unwrap_or("");
        let title = doc
            .query_selector_all_in_subtree(card, ".status-card__title")
            .first()
            .map(|&id| doc.text_content(id).trim().to_string())
            .unwrap_or_default();
        let description = doc
            .query_selector_all_in_subtree(card, ".status-card__description")
            .first()
            .map(|&id| doc.text_content(id).trim().to_string())
            .unwrap_or_default();
        if title.is_empty() && href.is_empty() {
            return String::new();
        }
        let escape = crate::content_utils::escape_html;
        let mut html = String::new();
        if let Some(src) = doc
            .query_selector_all_in_subtree(card, ".status-card__image-image")
            .first()
            .and_then(|id| doc.get_attr(*id, "src"))
            .filter(|src| !src.is_empty())
        {
            html.push_str(&format!(
                "<a href=\"{}\"><img src=\"{}\" alt=\"{}\" /></a>\n",
                escape(href),
                escape(src),
                escape(&title)
            ));
        }
        html.push_str(&format!(
            "<p><a href=\"{}\">{}</a></p>",
            escape(href),
            escape(if title.is_empty() { href } else { &title })
        ));
        if !description.is_empty() {
            html.push_str(&format!("\n<p>{}</p>", escape(&description)));
        }
        html
    }
}

impl MastodonExtractor {
    fn extract(&self, doc: &Document, page_url: &str) -> HtmlExtraction {
        let main_post = doc
            .query_selector_all(".detailed-status__wrapper")
            .first()
            .copied();

        let main_handle = main_post
            .map(|mp| short_handle(&full_handle(doc, mp)))
            .unwrap_or_default();

        let content_html = if let Some(main_id) = main_post {
            let mut post_parts: Vec<String> = Vec::new();
            let main_content = Self::post_content(doc, main_id);
            if !main_content.is_empty() {
                post_parts.push(main_content);
            }

            // Classify reply wrappers: consecutive self-replies at the start
            // form the thread, everything after forms comments.
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
                let wrapper_handle = short_handle(&full_handle(doc, wrapper));

                if !thread_ended && wrapper_handle == main_handle {
                    let thread_content = Self::post_content(doc, wrapper);
                    if !thread_content.is_empty() {
                        post_parts.push(thread_content);
                    }
                } else {
                    thread_ended = true;
                    comment_wrappers.push(wrapper);
                }
            }

            let post_content = post_parts.join("\n<hr>\n");

            let mut replies = Vec::new();
            let mut current_depth = 0;
            for (index, &wrapper) in comment_wrappers.iter().enumerate() {
                let handle = full_handle(doc, wrapper);

                let display_name = doc
                    .query_selector_all(".display-name__html")
                    .iter()
                    .find(|&&dn| doc.contains(wrapper, dn))
                    .map(|&dn| doc.text_content(dn).trim().to_string())
                    .unwrap_or_default();

                let reply_content = Self::post_content(doc, wrapper);
                if reply_content.is_empty() {
                    continue;
                }

                let date = doc
                    .query_selector_all("time[datetime]")
                    .iter()
                    .find(|&&t| doc.contains(wrapper, t))
                    .and_then(|&t| doc.get_attr(t, "datetime"))
                    .map(crate::content_utils::extract_date)
                    .unwrap_or_default();

                let permalink = doc
                    .query_selector_all("a.status__relative-time[href]")
                    .iter()
                    .find(|&&a| doc.contains(wrapper, a))
                    .and_then(|&a| doc.get_attr(a, "href"))
                    .map(|href| absolute_url(page_url, href))
                    .unwrap_or_default();

                // A reply starting a new top-level thread resets depth,
                // otherwise it nests one level deeper.
                if doc
                    .query_selector_all_in_subtree(wrapper, ".status--first-in-thread")
                    .is_empty()
                    && index != 0
                {
                    current_depth += 1;
                } else {
                    current_depth = 0;
                }

                let author = if !display_name.is_empty() {
                    format!("{display_name} @{handle}")
                } else {
                    format!("@{handle}")
                };

                replies.push(comments::CommentData {
                    author,
                    date,
                    content: reply_content,
                    depth: current_depth,
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
            .map(crate::content_utils::extract_date)
            .unwrap_or_default();

        let fallback_site = site_name.clone();
        let site = if site_name.is_empty() {
            String::new()
        } else {
            site_name
        };

        HtmlExtraction {
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

#[cfg(test)]
mod tests {
    use super::MastodonExtractor;
    use crate::dom::Document;

    fn status_page() -> Document {
        Document::parse(
            r#"<html><body>
            <div class="detailed-status__wrapper">
                <span class="display-name"><span class="display-name__html">Alice</span><span class="display-name__account">@alice@mastodon.example</span></span>
                <div class="status__content"><div class="status__content__text"><p>Main post <a href="https://example.com/x"><span class="invisible">https://</span><span>example.com/x</span></a></p></div></div>
            </div>
            <div class="status__wrapper"><div class="status" data-id="2">
                <span class="display-name__account">@alice</span>
                <div class="status__content__text"><p>Thread continuation</p></div>
            </div></div>
            <div class="status__wrapper"><div class="status status--first-in-thread" data-id="3">
                <span class="display-name__html">Bob</span><span class="display-name__account">@bob@other.social</span>
                <a class="status__relative-time" href="/@bob/3"></a><time datetime="2026-04-20T12:15:00.000Z"></time>
                <div class="status__content__text"><p>Top-level reply</p></div>
                <a class="status-card" href="https://example.com/card"><span class="status-card__content"><strong class="status-card__title">Card title</strong><span class="status-card__description">Card description</span></span></a>
            </div></div>
            </body></html>"#,
        )
    }

    #[test]
    fn self_reply_joins_thread_and_hides_url_scheme() {
        let doc = status_page();
        let extraction =
            MastodonExtractor.extract(&doc, "https://mastodon.example/@alice/1");
        let thread_end = extraction
            .content_html
            .find("## Comments")
            .unwrap_or(usize::MAX);
        let thread_pos = extraction
            .content_html
            .find("Thread continuation")
            .expect("thread reply extracted");
        assert!(
            thread_pos < thread_end,
            "self-reply joins the main post, not the comments"
        );
        assert!(!extraction.content_html.contains("invisible"));
        assert!(extraction.content_html.contains("example.com/x</span></a>"));
    }

    #[test]
    fn reply_keeps_card_and_absolute_permalink() {
        let doc = status_page();
        let extraction =
            MastodonExtractor.extract(&doc, "https://mastodon.example/@alice/1");
        assert!(extraction.content_html.contains("Card title"));
        assert!(extraction.content_html.contains("Card description"));
        assert!(
            extraction
                .content_html
                .contains("https://mastodon.example/@bob/3"),
            "relative permalink resolves against the page origin"
        );
    }
}
