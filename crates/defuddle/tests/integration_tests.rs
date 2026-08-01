use defuddle::markdown::html_to_markdown;

#[test]
fn parse_html_profiled_reports_pipeline_phases() {
    let html = "<article><h1>Profiled article</h1><p>This paragraph contains enough text to exercise the extraction and Markdown pipeline.</p></article>";
    let (result, profile) = defuddle::parse_html_profiled(
        html,
        defuddle::DefuddleOptions {
            separate_markdown: true,
            ..Default::default()
        },
    )
    .expect("profiled parse succeeds");

    assert!(result.content_markdown.is_some());
    assert!(profile.total_nanos() > 0);
    assert!(profile.phases.iter().any(|phase| phase.name == "parsing"));
    assert!(
        profile
            .phases
            .iter()
            .any(|phase| phase.name == "markdown convert")
    );
}

/// Direct tests of the standalone markdown conversion function,
/// mirroring tests/markdown.test.ts
mod markdown_conversion {
    use super::*;

    #[test]
    fn wraps_link_destinations_with_spaces_in_angle_brackets() {
        let html = r#"<article><p><a href="myapp:open shared page">Open page</a></p></article>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("[Open page](<myapp:open shared page>)"));
    }

    #[test]
    fn keeps_normal_link_destinations() {
        let html = r#"<article><p><a href="https://example.com/page">Open page</a></p></article>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("[Open page](https://example.com/page)"));
    }

    #[test]
    fn omits_image_alt_text_but_preserves_link_wrapping() {
        let html = r#"<p>Hello world</p><a href="https://example.com/image"><img src="https://example.com/img.png" alt="photo"></a>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("[![](https://example.com/img.png)](https://example.com/image)"));
        assert!(!md.contains("photo"));
    }

    #[test]
    fn removes_wbr_tags_without_spaces() {
        let html = r#"<article><p>Super<wbr>cali<wbr>fragilistic</p></article>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("Supercalifragilistic"));
    }

    #[test]
    fn handles_wbr_inside_links() {
        let html = r#"<p><a href="https://example.com">long<wbr>word</a></p>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("longword"));
    }

    #[test]
    fn converts_basic_tables() {
        let html = r#"<article>
            <table>
                <tr><th>A</th><th>B</th></tr>
                <tr><td>1</td><td>2</td></tr>
            </table>
        </article>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("| A | B |"));
        assert!(md.contains("| 1 | 2 |"));
    }
}

#[test]
fn preserves_video_elements_that_use_source_children() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Video With Source</title></head>
<body>
<article>
<h1>Video With Source</h1>
<p>This article includes a real video element.</p>
<video controls poster="https://example.com/poster.jpg">
    <source src="https://example.com/video.mp4" type="video/mp4">
</video>
</article>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com/video-with-source".into()),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    let content = result.content.to_lowercase();
    assert!(
        content.contains("<video"),
        "video element should be preserved"
    );
    assert!(
        content.contains("<source"),
        "source element should be preserved"
    );
    assert!(
        content.contains("video.mp4"),
        "video URL should be preserved"
    );
}

#[test]
fn standardizes_callouts_without_code_block_rules() {
    let html = r#"<!DOCTYPE html>
<html><head><title>Callout test</title></head><body><article>
<p>This article contains enough ordinary prose for the extractor to select the article as its main content.</p>
<div class="alert alert-info"><h4 class="alert-heading">Important</h4><p>Read this carefully before continuing.</p></div>
</article></body></html>"#;
    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            separate_markdown: true,
            url: Some("https://example.com/callout".into()),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    let markdown = result.content_markdown.expect("markdown output");
    assert!(markdown.contains("> [!info] Important"));
    assert!(markdown.contains("> Read this carefully before continuing."));
}

#[test]
fn strips_style_inside_svg() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>SVG style leak</title></head>
<body>
<article>
<h1>SVG style leak</h1>
<p>Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.</p>
<svg viewBox="0 0 200 200" width="200" height="200">
<style>@import url("http://attacker.example/leak.css"); .x { fill: url("http://attacker.example/img.png"); }</style>
<circle class="x" cx="100" cy="100" r="80"/>
</svg>
<p>More content here.</p>
</article>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com/svg-style".into()),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(
        !result.content.contains("@import"),
        "@import should be stripped"
    );
    assert!(
        !result.content.contains("attacker.example"),
        "attacker URL should be stripped"
    );
    assert!(
        !result.content.to_lowercase().contains("<style"),
        "style elements should be stripped"
    );
}

#[test]
fn markdown_true_converts_content() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test Page</title></head>
<body>
    <article>
        <h1>Test Article</h1>
        <p>This is a <strong>test</strong> paragraph.</p>
        <p>Another paragraph here.</p>
    </article>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            separate_markdown: true,
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(result.content.contains("<p>"), "content should be HTML");
    assert!(
        result.content.contains("<strong>"),
        "content should contain strong"
    );

    let md = result.content_markdown.unwrap_or_default();
    assert!(!md.is_empty(), "markdown should not be empty");
    assert!(!md.contains("<p>"), "markdown should not contain HTML");
    assert!(md.contains("**test**"), "markdown should contain bold");
}

#[test]
fn without_markdown_options_no_markdown_conversion() {
    let html = r#"<html><head><title>Test</title></head><body><p>Test</p></body></html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            separate_markdown: false,
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(result.content.contains("<p>"), "content should be HTML");
    assert!(
        result.content_markdown.is_none(),
        "content_markdown should be None"
    );
}

#[test]
fn parse_does_not_mutate_input() {
    // Our parse_html takes &str, so the input is always immutable.
    // This test confirms that parsing a document yields valid content
    // without panicking.
    let html = r#"
<!DOCTYPE html>
<html>
<head><title>Live Page</title></head>
<body>
    <style id="page-style">.headline { color: red }</style>
    <article>
        <h1 class="headline">Real article headline</h1>
        <p>This is a normal article with enough words for the content scorer to
        extract it as the main content without falling back to anything. It keeps
        going with several sentences so the word count is comfortably high.</p>
        <p>A second paragraph adds more text so extraction succeeds on the first try.</p>
        <script>console.log('inline script')</script>
    </article>
</body>
</html>"#;

    let original = html.to_string();
    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert_eq!(html, original, "input HTML must not be mutated");
    assert!(
        result.content.contains("second paragraph"),
        "content should be extracted"
    );
}

#[test]
fn debug_true_returns_debug_info() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body><article><h1>Title</h1><p>Content here.</p></article></body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            debug: true,
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(result.debug.is_some(), "debug should be present");
    let debug = result.debug.unwrap();
    assert!(
        !debug.content_selector.is_empty(),
        "content_selector should be set"
    );
}

#[test]
fn debug_false_does_not_include_debug_field() {
    let html = r#"<html><head><title>Test</title></head><body><p>Content.</p></body></html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            debug: false,
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(result.debug.is_none(), "debug should be None");
}

#[test]
fn content_selector_selects_specified_element() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
    <nav><a href="/">Home</a></nav>
    <main>
        <article><h1>Article</h1><p>Article content.</p></article>
    </main>
    <footer>Copyright</footer>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            content_selector: Some("article".into()),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(result.content.contains("Article content."));
    assert!(!result.content.contains("Home"));
    assert!(!result.content.contains("Copyright"));
}

#[test]
fn content_selector_falls_back_to_auto_detection() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body><article><p>Some content here that should be found.</p></article></body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            content_selector: Some(".nonexistent-class-xyz".into()),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(
        result.content.contains("Some content"),
        "should fall back to auto-detection"
    );
}

#[test]
fn remove_low_scoring_false_produces_more_content() {
    let html = r#"<!DOCTYPE html>
<html><head><title>Test</title></head>
<body><article><h1>Title</h1><p>Content that should be extracted normally.</p></article></body>
</html>"#;

    let defaults = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            ..Default::default()
        },
    )
    .expect("parse");

    let all_off = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            remove_low_scoring: Some(false),
            remove_hidden_elements: Some(false),
            remove_exact_selectors: Some(false),
            remove_partial_selectors: Some(false),
            ..Default::default()
        },
    )
    .expect("parse");

    assert!(
        all_off.content.len() >= defaults.content.len(),
        "all-off should produce >= content"
    );
}

#[test]
fn strips_script_tags_from_content() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<article>
<h1>Title</h1>
<p>Safe content.</p>
<script>alert('xss')</script>
</article>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            debug: true,
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(
        result.content.contains("Safe content"),
        "content should contain safe text"
    );
    assert!(
        !result.content.to_lowercase().contains("<script"),
        "script should be removed"
    );
    assert!(
        !result.content.contains("alert"),
        "script content should be removed"
    );
}

#[test]
fn strips_event_handlers() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<article>
<h1>Title</h1>
<p>Content.</p>
<img src="x.jpg" onerror="alert('xss')" onclick="steal()">
</article>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(
        !result.content.contains("onerror"),
        "onerror should be stripped"
    );
    assert!(
        !result.content.contains("onclick"),
        "onclick should be stripped"
    );
}

#[test]
fn strips_javascript_uris() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<article>
<p>Content.</p>
<a href="javascript:alert('xss')">click me</a>
<a href="  javascript:void(0)">spaced</a>
</article>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(
        !result.content.to_lowercase().contains("javascript:"),
        "javascript: URLs should be stripped"
    );
}

#[test]
fn strips_data_text_html_uris() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<article>
<p>Content.</p>
<img src="data:text/html,<script>alert(1)</script>">
</article>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(
        !result.content.to_lowercase().contains("data:text/html"),
        "data:text/html should be stripped"
    );
}

#[test]
fn preserves_iframes_with_src() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<article>
<p>Content.</p>
<iframe src="https://www.youtube.com/embed/abc123"></iframe>
<iframe src="https://open.spotify.com/embed/track/xyz"></iframe>
</article>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            remove_hidden_elements: Some(false),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(result.content.contains("youtube.com/embed/abc123"));
    assert!(result.content.contains("spotify.com/embed/track/xyz"));
}

#[test]
fn removes_iframes_when_requested() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<article>
<p>Content.</p>
<iframe src="https://substack.com/channel-frame"></iframe>
</article>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            remove_iframes: true,
            remove_hidden_elements: Some(false),
            separate_markdown: true,
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(!result.content.contains("substack.com/channel-frame"));
    assert!(
        !result
            .content_markdown
            .unwrap_or_default()
            .contains("substack.com/channel-frame")
    );
}

#[test]
fn strips_srcdoc_from_iframes() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<article>
<p>Content.</p>
<iframe srcdoc="<script>alert('xss')</script>"></iframe>
</article>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(
        !result.content.contains("srcdoc"),
        "srcdoc should be stripped"
    );
    assert!(
        !result.content.contains("alert"),
        "script content should be stripped"
    );
}

#[test]
fn strips_object_and_embed_elements() {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<article>
<p>Content.</p>
<object data="https://evil.com/flash.swf"></object>
<embed src="https://evil.com/plugin">
</article>
</body>
</html>"#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://example.com".into()),
            ..Default::default()
        },
    )
    .expect("parse succeeds");

    assert!(
        !result.content.to_lowercase().contains("<object"),
        "object should be stripped"
    );
    assert!(
        !result.content.to_lowercase().contains("<embed"),
        "embed should be stripped"
    );
    assert!(
        !result.content.contains("evil.com"),
        "evil URLs should be stripped"
    );
}

#[test]
fn hackernews_adapter_handles_listing_and_more_link() {
    let html = r#"
        <html><head><title>Hacker News</title></head><body>
        <table><tr class="athing" id="42">
          <td class="titleline"><a href="https://example.com/story">A story</a></td>
          <span class="sitestr">example.com</span>
        </tr><tr><td class="subtext"><span class="score">10 points</span>
          <a class="hnuser">alice</a><a href="item?id=42">3 comments</a>
        </td></tr></table>
        <a class="morelink" href="news?p=2">More</a>
        </body></html>
    "#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://news.ycombinator.com/news".into()),
            separate_markdown: true,
            ..Default::default()
        },
    )
    .expect("listing extraction succeeds");

    assert_eq!(result.extractor_type.as_deref(), Some("hackernews"));
    assert!(result.content.contains("<ol>"));
    assert!(
        result
            .content
            .contains("href=\"https://news.ycombinator.com/news?p=2\"")
    );
    assert!(
        result
            .content_markdown
            .unwrap_or_default()
            .contains("A story")
    );
}

#[test]
fn hackernews_adapter_handles_posts_without_comments() {
    let html = r#"
        <html><head><title>A post | Hacker News</title></head><body>
        <table class="fatitem"><tr class="athing" id="42">
          <td class="titleline"><a href="https://example.com/story">A post</a></td>
          <a class="hnuser">alice</a><span class="age" title="2025-01-15T12:00:00 1736942400">today</span>
          <div class="toptext"><p class="page-body" style="color:red" onclick="bad()">
            Post body. <a href="javascript:alert(1)">Unsafe link</a>
            <script>alert('bad')</script>
          </p></div>
        </tr></table>
        </body></html>
    "#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://news.ycombinator.com/item?id=42".into()),
            separate_markdown: true,
            ..Default::default()
        },
    )
    .expect("post extraction succeeds");

    assert_eq!(result.title, "A post");
    assert_eq!(result.author, "alice");
    assert!(!result.content.contains("Comments"));
    assert!(!result.content.contains("page-body"));
    assert!(!result.content.contains("style="));
    assert!(!result.content.contains("onclick"));
    assert!(!result.content.contains("javascript:"));
    assert!(!result.content.contains("<script"));
    assert!(
        result
            .content_markdown
            .unwrap_or_default()
            .contains("Post body.")
    );
}

#[test]
fn substack_adapter_keeps_content_dom_until_output() {
    let html = r#"
        <html><head>
          <meta property="og:title" content="A Substack post">
          <meta property="og:site_name" content="Example publication">
          <meta name="author" content="Ada Lovelace">
        </head><body>
          <div class="body markup">
            <p>Keep this paragraph.</p>
            <p class="button-wrapper">Remove this CTA.</p>
            <div data-component-name="SubscribeWidgetToDOM">Remove this widget.</div>
            <a href="/subscribe">Read full story</a>
          </div>
        </body></html>
    "#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            extractor: Some("substack".into()),
            url: Some("https://newsletter.example.com/p/post".into()),
            separate_markdown: true,
            ..Default::default()
        },
    )
    .expect("Substack extraction succeeds");

    assert_eq!(result.extractor_type.as_deref(), Some("substack"));
    assert_eq!(result.title, "A Substack post");
    assert_eq!(result.author, "Ada Lovelace");
    assert_eq!(result.site, "Example publication");
    assert!(result.content.contains("Keep this paragraph."));
    assert!(!result.content.contains("Remove this"));
    assert!(
        result
            .content_markdown
            .unwrap_or_default()
            .contains("Keep this paragraph.")
    );
}

#[test]
fn substack_adapter_reads_embedded_preload_content() {
    let html = r#"
        <html><head></head><body><script>
          window._preloads = true;
          JSON.parse("{\"feedData\":{\"initialPost\":{\"post\":{\"title\":\"Preloaded post\",\"publishedBylines\":[{\"name\":\"Grace Hopper\"}],\"body_html\":\"<p>Preloaded body.</p>\"}}}}")
        </script></body></html>
    "#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            extractor: Some("substack".into()),
            url: Some("https://substack.com/p/preloaded".into()),
            ..Default::default()
        },
    )
    .expect("Substack preload extraction succeeds");

    assert_eq!(result.title, "Preloaded post");
    assert_eq!(result.author, "Grace Hopper");
    assert!(result.content.contains("Preloaded body."));
}

#[test]
fn gmail_adapter_extracts_static_thread_messages() {
    let html = r#"
        <html><head><title>Fallback - account@example.com - Gmail</title></head><body>
          <h2 class="hP">Project kickoff</h2>
          <div class="adn ads">
            <span class="gD" name="Alex Rivera" email="alex@example.com">Alex Rivera</span>
            <span class="g3" title="May 13, 2026, 11:02 PM">date</span>
            <div class="a3s"><p>Hello Jane.</p></div>
          </div>
          <div class="adn ads">
            <span class="gD" name="Jane Doe" email="jane@example.com">Jane Doe</span>
            <span class="g3" title="May 14, 2026, 9:15 AM">date</span>
            <div class="a3s">
              <p>Thanks!</p>
              <div class="gmail_quote"><blockquote>Quoted history.</blockquote></div>
            </div>
          </div>
        </body></html>
    "#;

    let result = defuddle::parse_html(
        html,
        defuddle::DefuddleOptions {
            url: Some("https://mail.google.com/mail/u/0/#inbox/thread-f:1".into()),
            ..Default::default()
        },
    )
    .expect("Gmail extraction succeeds");

    assert_eq!(result.extractor_type.as_deref(), Some("gmail"));
    assert_eq!(result.title, "Project kickoff");
    assert_eq!(result.author, "Alex Rivera");
    assert_eq!(result.site, "Gmail");
    assert_eq!(result.published, "2026-05-13");
    assert!(result.content.contains("Hello Jane."));
    assert!(result.content.contains("Thanks!"));
    assert!(!result.content.contains("Quoted history."));
}
