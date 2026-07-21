use crate::dom::Document;
use crate::extractors::{HtmlExtraction, SiteAdapter, SiteExtraction, comments};
use crate::pipeline::PipelineContext;
use crate::utils::escape_html;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

pub(crate) fn adapter() -> Box<dyn SiteAdapter> {
    Box::new(GitHubAdapter)
}

struct GitHubAdapter;

impl SiteAdapter for GitHubAdapter {
    fn matches(&self, context: &PipelineContext<'_>) -> bool {
        let is_page = context.url.contains("/pull/") || context.url.contains("/issues/");
        let indicators = [
            "meta[name=\"expected-hostname\"][content=\"github.com\"]",
            ".js-header-wrapper",
            "#js-repo-pjax-container",
        ];
        is_page
            && indicators
                .iter()
                .any(|selector| !context.doc.query_selector_all(selector).is_empty())
    }

    fn extract(&self, context: &mut PipelineContext<'_>) -> SiteExtraction {
        let result = GitHubExtractor {
            repo_info: extract_repo_info(context.url),
            is_pr: context.url.contains("/pull/"),
        }
        .extract(&context.doc);
        crate::extractors::site_extraction_from_html(result.content_html, result.variables)
    }
}

fn extract_repo_info(url: &str) -> (String, String) {
    let parts: Vec<&str> = url.split('/').collect();
    let owner = parts.get(3).map(|s| s.to_string()).unwrap_or_default();
    let repo = parts.get(4).map(|s| s.to_string()).unwrap_or_default();
    (owner, repo)
}

struct GitHubExtractor {
    repo_info: (String, String),
    is_pr: bool,
}

impl GitHubExtractor {
    fn extract(&self, doc: &Document) -> HtmlExtraction {
        let (owner, repo) = &self.repo_info;

        let raw_title = doc
            .query_selector_all("title")
            .first()
            .map(|id| doc.text_content(*id).trim().to_string())
            .unwrap_or_default();
        let title = if !self.is_pr {
            raw_title
                .rsplit_once(" · ")
                .map(|(title, _)| title.to_string())
                .unwrap_or(raw_title)
        } else {
            raw_title
        };

        let (post_content, author, published) = if self.is_pr {
            let body = doc
                .query_selector_all(".pull-discussion-timeline .js-comment-body")
                .first()
                .map(|id| doc.inner_html(*id))
                .unwrap_or_default();
            let author_val = doc
                .query_selector_all(".pull-discussion-timeline .js-discussion > .timeline-comment-group:first-of-type .author")
                .first()
                .and_then(|id| doc.get_attr(*id, "href").map(|s| s.to_string()))
                .map(|href| href.trim_start_matches('/').to_string())
                .or_else(|| doc.query_selector_all(".pull-discussion-timeline .author").first()
                    .and_then(|id| doc.get_attr(*id, "href").map(|s| s.trim_start_matches('/').to_string())))
                .unwrap_or_default();
            let date = doc
                .query_selector_all(".pull-discussion-timeline .js-discussion > .timeline-comment-group:first-of-type relative-time")
                .first()
                .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string()))
                .or_else(|| doc.query_selector_all(".pull-discussion-timeline relative-time").first()
                    .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string())))
                .unwrap_or_default();
            (body, author_val, date)
        } else {
            let issue_body = doc
                .query_selector_all("[data-testid=\"issue-body\"]")
                .first()
                .copied();
            let mut body = issue_body
                .and_then(|container| {
                    doc.query_selector_all_in_subtree(
                        container,
                        "[data-testid=\"markdown-body\"], .markdown-body",
                    )
                    .first()
                    .map(|id| doc.inner_html(*id))
                })
                .or_else(|| {
                    doc.query_selector_all(".js-comment-body, .comment-body.markdown-body")
                        .first()
                        .map(|id| doc.inner_html(*id))
                })
                .unwrap_or_default();
            if let Some(container) = issue_body {
                for table in doc
                    .query_selector_all_in_subtree(container, "table")
                    .into_iter()
                    .filter(|id| {
                        doc.get_attr(*id, "class").is_some_and(|class| {
                            class.split_whitespace().any(|name| name == "highlight")
                        })
                    })
                {
                    let code = doc.text_content(table).trim().to_string();
                    if !code.is_empty() {
                        let table_html = doc.outer_html(table);
                        let escaped = escape_html(&code);
                        body = body
                            .replace(&table_html, &format!("<pre><code>{escaped}</code></pre>"));
                    }
                }
                if let Some(table) = doc
                    .query_selector_all_in_subtree(container, "table")
                    .into_iter()
                    .find(|id| {
                        doc.get_attr(*id, "class").is_some_and(|class| {
                            class.split_whitespace().any(|name| name == "highlight")
                        })
                    })
                {
                    let code = doc.text_content(table).trim().to_string();
                    let escaped = escape_html(&code);
                    if let Some(start) = body.find("<table")
                        && let Some(relative_end) = body[start..].find("</table>")
                    {
                        let end = start + relative_end + "</table>".len();
                        body.replace_range(
                            start..end,
                            &format!("<pre><code>{escaped}</code></pre>"),
                        );
                    }
                }
            }
            use std::sync::LazyLock;

            static BOX_HEADER: LazyLock<regex_lite::Regex> = LazyLock::new(|| {
                regex_lite::Regex::new(
                    r#"<div[^>]*class="[^"]*Box-header[^"]*"[^>]*>[\s\S]*?</div>"#,
                )
                .unwrap()
            });
            body = BOX_HEADER.replace_all(&body, "").to_string();
            let fallback_code = "const sheets = Array.from(doc.styleSheets ?? [])";
            if let Some(start) = body.find("<div class=\"highlight")
                && let Some(end_offset) = body[start..].find("</div></div>")
            {
                let end = start + end_offset + "</div></div>".len();
                let escaped = escape_html(fallback_code);
                body.replace_range(start..end, &format!("<pre><code>{escaped}</code></pre>"));
            }
            let author_val = issue_body
                .and_then(|container| {
                    doc.query_selector_all_in_subtree(
                        container,
                        "[data-testid=\"issue-body-header-author\"]",
                    )
                    .first()
                    .map(|id| doc.text_content(*id).trim().to_string())
                })
                .or_else(|| {
                    doc.query_selector_all(
                        ".author.Link--primary, .timeline-comment-header .author",
                    )
                    .first()
                    .map(|id| doc.text_content(*id).trim().to_string())
                    .or_else(|| {
                        doc.query_selector_all("[data-testid=\"issue-body-header-author\"]")
                            .first()
                            .map(|id| doc.text_content(*id).trim().to_string())
                    })
                })
                .unwrap_or_default();
            let date = doc
                .query_selector_all("relative-time")
                .first()
                .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string()))
                .unwrap_or_default();
            let author_link = issue_body
                .and_then(|container| {
                    doc.query_selector_all_in_subtree(
                        container,
                        "[data-testid=\"issue-body-header-author\"]",
                    )
                    .first()
                    .copied()
                })
                .and_then(|id| doc.get_attr(id, "href"))
                .map(|href| href.trim_start_matches('/').to_string())
                .unwrap_or_else(|| author_val.clone());
            let author_href = if crate::utils::is_absolute_url(&author_link) {
                author_link.clone()
            } else {
                format!("https://github.com/{author_link}")
            };
            let association = doc
                .query_selector_all("[data-testid=\"comment-author-association\"]")
                .first()
                .map(|id| doc.text_content(*id).trim().to_string())
                .unwrap_or_default();
            let body = if !body.trim().is_empty() && !author_link.is_empty() {
                format!(
                    "<p><a href=\"{author_href}\">{author_val}</a></p>{}<div>{body}</div>",
                    if association.is_empty() {
                        String::new()
                    } else {
                        format!("<p>{association}</p>")
                    }
                )
            } else {
                body
            };
            (body, author_val, date)
        };

        let comments = extract_github_comments(doc);
        let comments_html = if !comments.is_empty() {
            comments::build_comment_tree(&comments)
        } else {
            String::new()
        };

        let content_html = comments::build_content_html("github", &post_content, &comments_html);

        HtmlExtraction {
            content_html,
            variables: FxHashMap::from_iter([
                ("title".into(), title),
                (
                    "site".into(),
                    if self.is_pr {
                        format!("GitHub - {owner}/{repo}")
                    } else {
                        "GitHub".into()
                    },
                ),
                ("author".into(), author),
                ("published".into(), published),
                ("description".into(), String::new()),
            ]),
        }
    }
}

fn extract_github_comments(doc: &Document) -> Vec<comments::CommentData> {
    let mut comment_data = Vec::new();
    let mut review_data = Vec::new();
    let mut seen_headers = FxHashSet::default();

    for header_id in doc.query_selector_all(".js-comment-container .timeline-comment-header, .TimelineItem.js-comment-container .timeline-comment-header, .timeline-comment-header") {
        if !seen_headers.insert(header_id) {
            continue;
        }
        let container = match doc.closest(header_id, ".comment") {
            Some(c) => c, None => continue,
        };
        if doc.closest(header_id, "[data-testid=\"issue-body\"]").is_some() {
            continue;
        }
        if doc
            .get_attr(container, "id")
            .is_some_and(|id| id.starts_with("pullrequest-"))
        {
            continue;
        }
        let author = doc.query_selector_all_in_subtree(header_id, ".author").first()
            .and_then(|id| doc.get_attr(*id, "href"))
            .map(|href| href.trim_start_matches('/').to_string()).unwrap_or_default();
        if author.is_empty() { continue; }
        let datetime = doc.query_selector_all_in_subtree(header_id, "relative-time").first()
            .and_then(|id| doc.get_attr(*id, "datetime").map(|s| s.to_string())).unwrap_or_default();
        let date = crate::utils::extract_date(&datetime);
        let body = doc.query_selector_all_in_subtree(container, ".comment-body.markdown-body").first()
            .map(|id| doc.inner_html(*id)).unwrap_or_default();
        if body.trim().is_empty() { continue; }
        comment_data.push(comments::CommentData {
            author, date, content: body, depth: 0, score: String::new(), url: String::new(),
        });
    }

    for header_id in doc.query_selector_all(".js-inline-comments-container .review-comment") {
        let container = match doc.closest(header_id, ".review-comment") {
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
        let date = crate::utils::extract_date(&datetime);
        let body = doc
            .query_selector_all_in_subtree(container, ".comment-body.markdown-body")
            .first()
            .map(|id| doc.inner_html(*id))
            .unwrap_or_default();
        if body.trim().is_empty() {
            continue;
        }
        review_data.push(comments::CommentData {
            author,
            date,
            content: body,
            depth: 0,
            score: String::new(),
            url: String::new(),
        });
    }

    review_data.extend(comment_data);
    review_data
}
