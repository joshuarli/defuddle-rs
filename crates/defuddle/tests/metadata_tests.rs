use std::fs;
use std::path::{Path, PathBuf};

use deser::Deserialize;
use deser_value::Value;

use defuddle::{DefuddleOptions, parse_html};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream/defuddle/fixtures")
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream/defuddle/expected")
}

#[derive(Debug, Deserialize)]
struct ExpectedMetadata {
    title: String,
    author: String,
    site: String,
    published: String,
}

#[test]
fn metadata_extraction_on_simple_fixture() {
    let path = fixtures_dir().join("general--stephango.com-buy-wisely.html");
    let html = fs::read_to_string(&path).expect("fixture readable");

    let result = parse_html(
        &html,
        DefuddleOptions {
            url: Some("https://stephango.com/buy-wisely".to_string()),
            separate_markdown: true,
            ..<DefuddleOptions as Default>::default()
        },
    )
    .expect("parse succeeds");

    assert!(!result.title.is_empty(), "title should be extracted");
    assert!(!result.domain.is_empty(), "domain should be extracted");
    assert!(
        result.site.contains("stephango") || !result.site.is_empty(),
        "site should be extracted"
    );
}

#[test]
fn metadata_extraction_og_title() {
    let html = r#"
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta property="og:title" content="Open Graph Title">
        <meta name="description" content="A test description">
        <meta property="og:image" content="https://example.com/image.jpg">
        <title>Page Title</title>
    </head>
    <body>
        <h1>Heading</h1>
        <p>Some content here.</p>
    </body>
    </html>
    "#;

    let result = parse_html(
        html,
        DefuddleOptions {
            url: Some("https://example.com/page".to_string()),
            ..<DefuddleOptions as Default>::default()
        },
    )
    .expect("parse succeeds");

    assert_eq!(result.title, "Open Graph Title");
    assert_eq!(result.description, "A test description");
    assert_eq!(result.domain, "example.com");
    assert_eq!(result.image, "https://example.com/image.jpg");
}

#[test]
fn metadata_extraction_author_byline() {
    let html = r#"
    <!DOCTYPE html>
    <html>
    <head><title>Test Article</title></head>
    <body>
        <article>
            <p class="byline">By John Doe</p>
            <h1>Article Title</h1>
            <p>Content goes here.</p>
        </article>
    </body>
    </html>
    "#;

    let result = parse_html(
        html,
        DefuddleOptions {
            url: Some("https://example.com/article".to_string()),
            ..<DefuddleOptions as Default>::default()
        },
    )
    .expect("parse succeeds");

    assert_eq!(result.author, "John Doe");
}

#[test]
fn metadata_extraction_schema_org() {
    let html = r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>Schema Test</title>
        <script type="application/ld+json">
        {
            "@context": "https://schema.org",
            "@type": "Article",
            "headline": "Schema Headline",
            "author": {"name": "Jane Smith"},
            "datePublished": "2024-01-15T10:00:00Z",
            "publisher": {"name": "Example Publisher"}
        }
        </script>
    </head>
    <body>
        <h1>Schema Test</h1>
        <p>Test content.</p>
    </body>
    </html>
    "#;

    let result = parse_html(
        html,
        DefuddleOptions {
            url: Some("https://example.com/schema".to_string()),
            ..<DefuddleOptions as Default>::default()
        },
    )
    .expect("parse succeeds");

    assert!(!result.author.is_empty(), "author should be found");
    assert!(
        !result.published.is_empty(),
        "published date should be found"
    );
    assert!(
        !result.site.is_empty(),
        "site should be found from publisher"
    );
}

#[test]
fn schema_text_fallback_sanitizes_dangerous_markup() {
    // schema.org "text" is attacker-controlled page markup, not trusted HTML.
    // When it is long enough to replace a too-short extraction, it must be
    // sanitized before conversion to Markdown, same as any other extractor output.
    // `<` and `>` are JSON unicode-escaped so the embedded markup doesn't
    // prematurely close the surrounding `<script type="application/ld+json">`
    // tag during HTML tokenization.
    let dangerous_markup =
        r#"<script>alert(1)</script><img src=\"data:text/html,<script>alert(1)</script>\">"#
            .replace('<', "\\u003c")
            .replace('>', "\\u003e");
    let schema_text_json = "Real article body ".repeat(20) + &dangerous_markup;
    let html = format!(
        r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>Schema Fallback Test</title>
        <script type="application/ld+json">
        {{
            "@context": "https://schema.org",
            "@type": "Article",
            "headline": "Schema Fallback Test",
            "text": "{schema_text_json}"
        }}
        </script>
    </head>
    <body>
        <p>short</p>
    </body>
    </html>
    "#
    );

    let result = parse_html(
        &html,
        DefuddleOptions {
            url: Some("https://example.com/schema-fallback".to_string()),
            separate_markdown: true,
            ..<DefuddleOptions as Default>::default()
        },
    )
    .expect("parse succeeds");

    let markdown = result.content_markdown.expect("markdown produced");
    assert!(
        markdown.contains("Real article body"),
        "schema text should be used as fallback content"
    );
    assert!(
        !markdown.contains("data:text/html"),
        "dangerous data: URL from schema text must be sanitized"
    );
    assert!(
        !markdown.contains("alert(1)"),
        "script content from schema text must be sanitized"
    );
}

#[test]
fn metadata_language_extraction() {
    let html = r#"
    <!DOCTYPE html>
    <html lang="fr">
    <head><title>Test Français</title></head>
    <body><p>Contenu.</p></body>
    </html>
    "#;

    let result = parse_html(
        html,
        DefuddleOptions {
            url: Some("https://example.fr/page".to_string()),
            ..<DefuddleOptions as Default>::default()
        },
    )
    .expect("parse succeeds");

    assert_eq!(result.language, "fr");
}

#[test]
fn metadata_favicon_detection() {
    let html = r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>Favicon Test</title>
        <link rel="icon" href="/favicon.png">
    </head>
    <body><p>Content.</p></body>
    </html>
    "#;

    let result = parse_html(
        html,
        DefuddleOptions {
            url: Some("https://example.com/page".to_string()),
            ..<DefuddleOptions as Default>::default()
        },
    )
    .expect("parse succeeds");

    assert_eq!(result.favicon, "/favicon.png");
}

#[test]
fn metadata_favicon_uses_the_declared_icon_priority() {
    let regular = r#"<link rel="icon" href="/regular.svg">"#;
    let shortcut = r#"<link rel="shortcut icon" href="/shortcut.ico">"#;
    let open_graph = r#"<meta property="og:image:favicon" content="https://assets.example.com/og.svg">"#;
    for (head, expected) in [
        (format!("{shortcut}{regular}{open_graph}"), "https://assets.example.com/og.svg"),
        (format!("{shortcut}{regular}"), "/regular.svg"),
        (shortcut.to_string(), "/shortcut.ico"),
    ] {
        let html = format!("<html><head>{head}</head><body><article><p>Read the field notes.</p></article></body></html>");
        let result = parse_html(&html, DefuddleOptions::default()).expect("parse succeeds");
        assert_eq!(result.favicon, expected);
    }
}

#[test]
fn metadata_favicon_falls_back_to_the_document_origin() {
    let html = "<article><p>Read the field notes.</p></article>";
    for (url, expected) in [
        (Some("https://example.com/notes/field"), "https://example.com/favicon.ico"),
        (Some("http://example.com/notes/field"), "http://example.com/favicon.ico"),
        (Some("https://example.com"), "https://example.com/favicon.ico"),
        (Some("notes.html"), ""),
        (None, ""),
    ] {
        let result = parse_html(
            html,
            DefuddleOptions { url: url.map(str::to_string), ..DefuddleOptions::default() },
        )
        .expect("parse succeeds");
        assert_eq!(result.favicon, expected, "document URL {url:?}");
    }
}

#[test]
fn metadata_uses_the_declared_url_precedence() {
    let sources = [
        (r#"<meta property="OG:URL" content="https://graph.example/article">"#, "graph.example"),
        (r#"<meta property="twitter:url" content="https://social.example/article">"#, "social.example"),
        (r#"<script type="application/ld+json">{"@type":"Article","url":"https://schema.example/article"}</script>"#, "schema.example"),
        (r#"<link rel="canonical" href="https://canonical.example/article">"#, "canonical.example"),
        (r#"<base href="https://base.example/article">"#, "base.example"),
    ];
    let parse = |head: &str, url: Option<&str>| {
        let html = format!("<html><head>{head}</head><body><p>Field notes.</p></body></html>");
        parse_html(
            &html,
            DefuddleOptions { url: url.map(str::to_string), ..DefuddleOptions::default() },
        )
        .expect("parse succeeds")
    };
    let head: String = sources.iter().map(|(markup, _)| *markup).collect();
    let explicit = parse(&head, Some("http://www.request.example/notes"));
    assert_eq!(explicit.domain, "request.example");
    assert_eq!(explicit.favicon, "http://www.request.example/favicon.ico");
    for (index, (_, domain)) in sources.iter().enumerate() {
        let head: String = sources[index..].iter().map(|(markup, _)| *markup).collect();
        let result = parse(&head, None);
        assert_eq!(result.domain, *domain);
        assert_eq!(result.favicon, format!("https://{domain}/favicon.ico"));
    }
    let relative = parse(r#"<link rel="canonical" href="/notes"><base href="https://base.example/notebook/">"#, None);
    assert_eq!(relative.domain, "base.example");
    assert_eq!(relative.favicon, "https://base.example/favicon.ico");
    let absent = parse("", None);
    assert!(absent.domain.is_empty());
    assert!(absent.favicon.is_empty());
}

#[test]
fn metadata_date_published() {
    let html = r#"
    <!DOCTYPE html>
    <html>
    <head><title>Date Test</title></head>
    <body>
        <h1>Article with Date</h1>
        <time datetime="2024-06-15">June 15, 2024</time>
        <p>Article content here.</p>
    </body>
    </html>
    "#;

    let result = parse_html(
        html,
        DefuddleOptions {
            url: Some("https://example.com/dated".to_string()),
            ..<DefuddleOptions as Default>::default()
        },
    )
    .expect("parse succeeds");

    assert_eq!(result.published, "2024-06-15");
}

#[test]
fn metadata_published_prefers_the_first_usable_date_source() {
    let schema = r#"<script type="application/ld+json">{"@type":"Article","datePublished":"2025-01-01"}</script>"#;
    let publish_meta = r#"<meta name="publishDate" content="2025-02-02">"#;
    let open_graph = r#"<meta property="article:published_time" content="2025-03-03">"#;
    let fallback_meta = r#"<meta name="sailthru.date" content="2025-06-06">"#;
    let abbr = r#"<abbr itemprop="datePublished" title="2025-04-04">April 4</abbr>"#;
    let related = r#"<a href="/related"><time datetime="2024-09-09">Previous story</time></a>"#;
    let current = r#"<time datetime=" 2025-05-05 ">May 5</time>"#;
    for (head, body, expected) in [
        (format!("{schema}{publish_meta}{open_graph}{fallback_meta}"), format!("{abbr}{related}{current}"), "2025-01-01"),
        (format!("{publish_meta}{open_graph}{fallback_meta}"), format!("{abbr}{related}{current}"), "2025-02-02"),
        (format!("{open_graph}{fallback_meta}"), format!("{abbr}{related}{current}"), "2025-03-03"),
        (fallback_meta.to_string(), format!("{abbr}{related}{current}"), "2025-04-04"),
        (fallback_meta.to_string(), format!("{related}{current}"), "2025-05-05"),
        (fallback_meta.to_string(), related.to_string(), "2025-06-06"),
    ] {
        let html = format!("<html><head>{head}</head><body><article><h1>Field notes</h1>{body}<p>Read the notes before the walk.</p></article></body></html>");
        let result = parse_html(
            &html,
            DefuddleOptions { url: Some("https://example.com/article".into()), ..DefuddleOptions::default() },
        )
        .expect("parse succeeds");
        assert_eq!(result.published, expected, "{html}");
    }
}

#[test]
fn metadata_preamble_parity_for_sample_fixtures() {
    let fixtures = &["general--stephango.com-buy-wisely", "author-contact-block"];

    let mut preamble_matches = 0;

    for name in fixtures {
        let html_path = fixtures_dir().join(format!("{name}.html"));
        let expected_path = expected_dir().join(format!("{name}.md"));

        let html = fs::read_to_string(&html_path).expect("fixture readable");
        if !expected_path.exists() {
            continue;
        }

        let expected = fs::read_to_string(&expected_path).expect("expected readable");
        let expected_meta = parse_fixture_preamble(&expected);

        let result = parse_html(
            &html,
            DefuddleOptions {
                url: Some(fixture_url_from_path(&html, &html_path)),
                separate_markdown: true,
                ..<DefuddleOptions as Default>::default()
            },
        )
        .expect("parse succeeds");

        if let Some(em) = expected_meta {
            if result.title == em.title
                && result.author == em.author
                && result.site == em.site
                && result.published == em.published
            {
                preamble_matches += 1;
            } else {
                eprintln!("Mismatch for {name}:");
                eprintln!("  title:     {:?} vs {:?}", result.title, em.title);
                eprintln!("  author:    {:?} vs {:?}", result.author, em.author);
                eprintln!("  site:      {:?} vs {:?}", result.site, em.site);
                eprintln!("  published: {:?} vs {:?}", result.published, em.published);
            }
        }
    }

    assert!(
        preamble_matches >= 1,
        "at least one fixture should match metadata preamble, got {preamble_matches}"
    );
}

fn fixture_url_from_path(html: &str, path: &Path) -> String {
    if let Some(start) = html.find("<!--")
        && let Some(end_offset) = html[start + 4..].find("-->")
    {
        let comment = html[start + 4..start + 4 + end_offset].trim();
        if comment.starts_with('{')
            && let Ok(value) = deser_json::from_str::<Value>(comment)
            && let Some(url) = value.get("url").and_then(|url| url.as_str())
        {
            return url.to_string();
        }
    }

    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("fixture has UTF-8 stem");
    let url_name = stem.split_once("--").map(|(_, rest)| rest).unwrap_or(stem);
    format!("https://{url_name}")
}

fn parse_fixture_preamble(expected: &str) -> Option<ExpectedMetadata> {
    let json_start = expected.find("```json")? + 7;
    let json_end = expected[json_start..].find("```")?;
    let json_str = &expected[json_start..json_start + json_end];
    deser_json::from_str::<ExpectedMetadata>(json_str).ok()
}
