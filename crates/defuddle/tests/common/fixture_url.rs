use std::path::Path;

use miniserde::json::Value;

pub fn fixture_url(html: &str, path: &Path) -> String {
    if let Some(start) = html.find("<!--")
        && let Some(end_offset) = html[start + 4..].find("-->")
    {
        let comment = html[start + 4..start + 4 + end_offset].trim();
        if comment.starts_with('{')
            && let Ok(value) = miniserde::json::from_str::<Value>(comment)
            && let Value::Object(object) = value
            && let Some(Value::String(url)) = object.get("url")
        {
            return url.to_string();
        }
    }

    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("fixture has UTF-8 stem");
    let named_urls = [
        (
            "general--github.com-issue-56",
            "https://github.com/kepano/defuddle/issues/56",
        ),
        (
            "general--github.com-test-owner-test-repo-pull-42",
            "https://github.com/test-owner/test-repo/pull/42",
        ),
        (
            "comments--news.ycombinator.com-item-id=12345678",
            "https://news.ycombinator.com/item?id=12345678",
        ),
        (
            "general--news.ycombinator.com-item-id=12345678",
            "https://news.ycombinator.com/item?id=12345678",
        ),
        (
            "listing--news.ycombinator.com-news",
            "https://news.ycombinator.com/news",
        ),
        (
            "comments--old.reddit.com-r-test-comments-abc123-test_post",
            "https://old.reddit.com/r/test/comments/abc123/test_post",
        ),
        (
            "extractor--chatgpt-citations",
            "https://chatgpt.com/share/citations",
        ),
        (
            "extractor--chatgpt-post-thought-content",
            "https://chatgpt.com/share/thoughts",
        ),
        ("general--substack-app", "https://substack.com/app"),
        (
            "general--substack-custom-domain",
            "https://example.substack.com/p/test-article",
        ),
        ("general--x.com-article", "https://x.com/article"),
        (
            "general--x.com-article-2026-02-13",
            "https://x.com/article/2026-02-13",
        ),
        (
            "content-patterns--content-list-with-internal-links",
            "https://content-patterns--content-list-with-internal-links",
        ),
    ];
    if let Some((_, url)) = named_urls.iter().find(|(name, _)| *name == stem) {
        return (*url).to_string();
    }
    if stem == "table-layout--single-column" {
        return "https://table-layout--single-column".to_string();
    }
    let url_name = stem.split_once("--").map(|(_, rest)| rest).unwrap_or(stem);
    if url_name.contains("github.com")
        || url_name.contains("news.ycombinator.com")
        || url_name.contains("reddit.com")
        || url_name.contains("mastodon.")
        || url_name.contains("chatgpt")
        || url_name.contains("substack")
        || url_name.contains("wikipedia")
        || url_name.contains("figma.com")
        || url_name.contains("mozilla.org")
        || url_name.contains("x.com")
        || url_name.contains("twitter.com")
        || url_name.contains("131-category-links")
        || url_name.contains("168-links")
    {
        return format!("https://{url_name}");
    }
    String::new()
}
