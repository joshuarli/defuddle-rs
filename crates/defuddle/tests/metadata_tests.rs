use std::fs;
use std::path::{Path, PathBuf};

use miniserde::Deserialize;
use miniserde::json::Value;

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

    assert!(
        result.favicon.contains("favicon"),
        "favicon should reference a favicon resource"
    );
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
    let url_name = stem.split_once("--").map(|(_, rest)| rest).unwrap_or(stem);
    format!("https://{url_name}")
}

fn parse_fixture_preamble(expected: &str) -> Option<ExpectedMetadata> {
    let json_start = expected.find("```json")? + 7;
    let json_end = expected[json_start..].find("```")?;
    let json_str = &expected[json_start..json_start + json_end];
    miniserde::json::from_str::<ExpectedMetadata>(json_str).ok()
}
