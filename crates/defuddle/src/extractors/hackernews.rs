use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

use crate::dom::{Document, NodeId};
use crate::extractors::dom_builder::Builder;
use crate::extractors::{SiteAdapter, SiteExtraction};
use crate::pipeline::ExtractionContext;

pub(crate) fn adapter() -> Box<dyn SiteAdapter> {
    Box::new(HackerNewsAdapter)
}

struct HackerNewsAdapter;

impl SiteAdapter for HackerNewsAdapter {
    fn matches(&self, context: &ExtractionContext<'_>) -> bool {
        let main_post = !context.doc.query_selector_all(".fatitem").is_empty();
        let listing = !context.doc.query_selector_all("tr.athing").is_empty() && !main_post;
        main_post || listing
    }

    fn extract(&self, context: &mut ExtractionContext<'_>) -> SiteExtraction {
        let main_post = !context.doc.query_selector_all(".fatitem").is_empty();
        let is_comment_page = main_post && {
            let post = context.doc.query_selector_all(".fatitem")[0];
            !context
                .doc
                .query_selector_all_in_subtree(post, ".onstory")
                .is_empty()
                && context
                    .doc
                    .query_selector_all_in_subtree(post, ".titleline")
                    .is_empty()
        };

        if !main_post {
            return extract_listing(&context.doc);
        }
        extract_post(&context.doc, is_comment_page)
    }
}

fn extract_listing(doc: &Document) -> SiteExtraction {
    let mut builder = Builder::new();
    let ol = builder.element(builder.root, "ol", &[]);

    for athing in doc.query_selector_all("tr.athing") {
        let title_id = doc
            .query_selector_all_in_subtree(athing, ".titleline a")
            .first()
            .copied();
        let title_link = title_id
            .and_then(|id| doc.get_attr(id, "href"))
            .unwrap_or_default();
        let title_url = absolute_hn_url(title_link);
        let title_text = title_id
            .map(|id| doc.text_content(id).trim().to_string())
            .unwrap_or_default();
        let site = doc
            .query_selector_all_in_subtree(athing, ".sitestr")
            .first()
            .map(|id| doc.text_content(*id).trim().to_string())
            .unwrap_or_default();

        let subtext = next_subtext(doc, athing);
        let score = subtext
            .and_then(|id| {
                doc.query_selector_all_in_subtree(id, ".score")
                    .first()
                    .copied()
            })
            .map(|id| doc.text_content(id).trim().to_string())
            .unwrap_or_default();
        let author = subtext
            .and_then(|id| {
                doc.query_selector_all_in_subtree(id, ".hnuser")
                    .first()
                    .copied()
            })
            .map(|id| doc.text_content(id).trim().to_string())
            .unwrap_or_default();
        let item_id = doc.get_attr(athing, "id").unwrap_or_default();
        let comments = subtext.and_then(|id| {
            doc.query_selector_all_in_subtree(id, "a")
                .into_iter()
                .find(|&link| doc.text_content(link).contains("comments"))
                .map(|link| doc.text_content(link).trim().replace('\u{a0}', " "))
        });

        let li = builder.element(ol, "li", &[]);
        let title = builder.element(li, "a", &[("href", &title_url)]);
        builder.text(title, &title_text);
        if !site.is_empty() {
            builder.text(li, &format!(" ({site})"));
        }
        let detail = builder.element(li, "div", &[]);
        builder.text(detail, &format!("{score} · by {author}"));
        if let Some(comments) = comments {
            builder.text(detail, " · ");
            let item_url = if item_id.is_empty() {
                title_url.clone()
            } else {
                format!("https://news.ycombinator.com/item?id={item_id}")
            };
            let link = builder.element(detail, "a", &[("href", &item_url)]);
            builder.text(link, &comments);
        }
    }

    if let Some(more) = doc.query_selector_all(".morelink").first().copied()
        && let Some(href) = doc.get_attr(more, "href")
        && !href.is_empty()
    {
        let url = absolute_hn_url(href);
        builder.text(builder.root, "\n\n");
        let link = builder.element(builder.root, "a", &[("href", &url)]);
        builder.text(link, doc.text_content(more).trim());
    }

    let title = page_title(doc);
    let content_root = builder.root;
    SiteExtraction {
        extracted_content: builder.finish(content_root),
        variables: FxHashMap::from_iter([
            ("title".into(), title),
            ("site".into(), "Hacker News".into()),
        ]),
    }
}

fn extract_post(doc: &Document, is_comment_page: bool) -> SiteExtraction {
    let mut builder = Builder::new();
    let article = builder.element(builder.root, "article", &[("data-defuddle", "")]);
    let post = builder.element(article, "div", &[("class", "hackernews post")]);
    let post_content = builder.element(post, "div", &[("class", "post-content")]);

    if is_comment_page {
        if let Some(comment) = doc.query_selector_all("tr.athing").first().copied() {
            append_comment(&mut builder, doc, post_content, comment, 0, "", false);
        }
    } else {
        let title_link = doc.query_selector_all(".titleline a").first().copied();
        if let Some(title_link) = title_link {
            let href = doc.get_attr(title_link, "href").unwrap_or_default();
            let link = builder.element(post_content, "p", &[]);
            let anchor = builder.element(link, "a", &[("href", href), ("target", "_blank")]);
            builder.text(anchor, href);
        }
        if let Some(toptext) = doc.query_selector_all(".toptext").first().copied() {
            let wrapper = builder.element(post_content, "div", &[("class", "post-text")]);
            builder.clone_children(doc, toptext, wrapper);
        }
    }

    let comments = collect_comments(doc);
    if !comments.is_empty() {
        builder.text(article, "\n\t\t\t");
        builder.element(article, "hr", &[]);
        builder.text(article, "\n\t\t\t");
        let section = builder.element(article, "div", &[("class", "hackernews comments")]);
        builder.text(section, "\n\t\t\t\t");
        builder.element(section, "h2", &[]);
        let heading = section_children_last(&builder.document, section);
        if let Some(heading) = heading {
            builder.text(heading, "Comments");
        }
        builder.text(section, "\n\t\t\t\t");
        append_comment_tree(&mut builder, doc, section, &comments);
        builder.text(section, "\n\t\t\t");
    }
    let title = post_title(doc, is_comment_page);
    let author = post_author(doc);
    let published = post_date(doc);
    SiteExtraction {
        extracted_content: builder.finish(article),
        variables: FxHashMap::from_iter([
            ("title".into(), title.clone()),
            ("author".into(), author.clone()),
            ("site".into(), "Hacker News".into()),
            (
                "description".into(),
                format!("{title} - by {author} on Hacker News"),
            ),
            ("published".into(), published),
        ]),
    }
}

fn section_children_last(doc: &Document, section: NodeId) -> Option<NodeId> {
    doc.children(section)
        .iter()
        .rev()
        .copied()
        .find(|&id| doc.tag_name_is(id, "h2"))
}

fn append_comment_tree(
    builder: &mut Builder,
    doc: &Document,
    parent: NodeId,
    comments: &[Comment],
) {
    let mut stack: Vec<(u32, NodeId)> = Vec::new();
    for comment in comments {
        if comment.depth == 0 && !stack.is_empty() {
            stack.clear();
            builder.text(parent, "\n");
        }
        while stack
            .last()
            .is_some_and(|(depth, _)| *depth > comment.depth)
        {
            stack.pop();
        }
        let blockquote = if let Some((depth, node)) = stack.last() {
            if *depth == comment.depth {
                *node
            } else {
                let parent = stack.last().map(|(_, node)| *node).unwrap_or(parent);
                let blockquote = builder.element(parent, "blockquote", &[]);
                stack.push((comment.depth, blockquote));
                blockquote
            }
        } else {
            let blockquote = builder.element(parent, "blockquote", &[]);
            stack.push((comment.depth, blockquote));
            blockquote
        };
        append_comment(
            builder,
            doc,
            blockquote,
            comment.node,
            comment.depth,
            &comment.date,
            true,
        );
    }
}

fn append_comment(
    builder: &mut Builder,
    doc: &Document,
    parent: NodeId,
    comment: NodeId,
    _depth: u32,
    date: &str,
    with_link: bool,
) {
    let wrapper = builder.element(parent, "div", &[("class", "comment")]);
    let metadata = builder.element(wrapper, "div", &[("class", "comment-metadata")]);
    let author = doc
        .query_selector_all_in_subtree(comment, ".hnuser")
        .first()
        .map(|id| doc.text_content(*id).trim().to_string())
        .unwrap_or_else(|| "[deleted]".into());
    let author_span = builder.element(metadata, "span", &[("class", "comment-author")]);
    let strong = builder.element(author_span, "strong", &[]);
    builder.text(strong, &author);
    builder.text(metadata, " · ");
    let date_value = if date.is_empty() {
        comment_date(doc, comment)
    } else {
        date.to_string()
    };
    if with_link {
        let id = doc.get_attr(comment, "id").unwrap_or_default();
        let url = format!("https://news.ycombinator.com/item?id={id}");
        let link = builder.element(metadata, "a", &[("href", &url), ("class", "comment-link")]);
        builder.text(link, &date_value);
    } else {
        let span = builder.element(metadata, "span", &[("class", "comment-date")]);
        builder.text(span, &date_value);
    }
    if let Some(score) = doc
        .query_selector_all_in_subtree(comment, ".score")
        .first()
        .copied()
    {
        builder.text(metadata, " · ");
        let points = builder.element(metadata, "span", &[("class", "comment-points")]);
        builder.text(points, doc.text_content(score).trim());
    }
    let content = builder.element(wrapper, "div", &[("class", "comment-content")]);
    if let Some(commtext) = doc
        .query_selector_all_in_subtree(comment, ".commtext")
        .first()
        .copied()
    {
        builder.clone_children(doc, commtext, content);
    }
}

struct Comment {
    node: NodeId,
    depth: u32,
    date: String,
}

fn collect_comments(doc: &Document) -> Vec<Comment> {
    let mut seen = FxHashSet::default();
    doc.query_selector_all("tr.comtr")
        .into_iter()
        .filter_map(|node| {
            let id = doc.get_attr(node, "id")?.to_string();
            if !seen.insert(id)
                || doc
                    .query_selector_all_in_subtree(node, ".commtext")
                    .is_empty()
            {
                return None;
            }
            let width = doc
                .query_selector_all_in_subtree(node, ".ind img")
                .first()
                .and_then(|id| doc.get_attr(*id, "width"))
                .and_then(|width| width.parse::<u32>().ok())
                .unwrap_or(0);
            Some(Comment {
                node,
                depth: width / 40,
                date: comment_date(doc, node),
            })
        })
        .collect()
}

fn next_subtext(doc: &Document, node: NodeId) -> Option<NodeId> {
    let mut next = doc.next_element_sibling(node);
    for _ in 0..3 {
        if next.is_some_and(|id| !doc.query_selector_all_in_subtree(id, ".subtext").is_empty()) {
            return next;
        }
        next = next.and_then(|id| doc.next_element_sibling(id));
    }
    None
}

fn absolute_hn_url(url: &str) -> String {
    if crate::content_utils::is_absolute_url(url) {
        url.to_string()
    } else {
        format!(
            "https://news.ycombinator.com/{}",
            url.trim_start_matches('/')
        )
    }
}

fn page_title(doc: &Document) -> String {
    doc.query_selector_all("title")
        .first()
        .map(|id| {
            let title = doc.text_content(*id);
            title
                .trim()
                .strip_suffix(" | Hacker News")
                .unwrap_or(title.trim())
                .trim()
                .to_string()
        })
        .unwrap_or_else(|| "Hacker News".into())
}

fn post_title(doc: &Document, comment_page: bool) -> String {
    if comment_page {
        let author = post_author(doc);
        let text = doc
            .query_selector_all(".commtext")
            .first()
            .map(|id| doc.text_content(*id).trim().to_string())
            .unwrap_or_default();
        let preview: String = text.chars().take(50).collect();
        return format!(
            "Comment by {author}: {}",
            if text.chars().count() > 50 {
                format!("{preview}...")
            } else {
                text
            }
        );
    }
    doc.query_selector_all(".titleline")
        .first()
        .map(|id| doc.text_content(*id).trim().to_string())
        .unwrap_or_default()
}

fn post_author(doc: &Document) -> String {
    doc.query_selector_all(".hnuser")
        .first()
        .map(|id| doc.text_content(*id).trim().to_string())
        .unwrap_or_default()
}

fn post_date(doc: &Document) -> String {
    doc.query_selector_all(".age")
        .first()
        .and_then(|id| doc.get_attr(*id, "title"))
        .map(crate::content_utils::extract_date)
        .unwrap_or_default()
}

fn comment_date(doc: &Document, comment: NodeId) -> String {
    doc.query_selector_all_in_subtree(comment, ".age")
        .first()
        .and_then(|id| doc.get_attr(*id, "title"))
        .map(crate::content_utils::extract_date)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::Document;
    use crate::metadata::Metadata;

    #[test]
    fn adapter_keeps_dom_until_output_boundary() {
        let source = Document::parse(
            r#"<html><body><table><tr class="athing" id="1"><td class="titleline"><a href="/item?id=1">Story</a></td></tr></table></body></html>"#,
        );
        let options = crate::DefuddleOptions::default();
        let metadata = Metadata::default();
        let schema = Vec::new();
        let meta_tags = Vec::new();
        Document::reset_test_counters();
        let mut context = ExtractionContext {
            source_html: "",
            doc: source,
            metadata: &metadata,
            meta_tags: &meta_tags,
            schema_org_data: &schema,
            page_url: "https://news.ycombinator.com/news",
            defuddle_options: &options,
            diagnostics: Default::default(),
            is_simple_redirect: false,
        };

        let extraction = adapter().extract(&mut context);
        assert_eq!(Document::test_parse_count(), 0);
        let _ = extraction.extracted_content.html();
        assert_eq!(Document::test_parse_count(), 0);
        assert_eq!(Document::test_serialize_count(), 1);
    }
}
