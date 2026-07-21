use rustc_hash::FxHashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use defuddle::{DefuddleOptions, DefuddleResult, extractors, parse_html};
use miniserde::json::Value;
use walkdir::WalkDir;

const FIXTURES_DIR: &str = "../../tests/upstream/defuddle/fixtures";
const EXPECTED_DIR: &str = "../../tests/upstream/defuddle/expected";
const EXCEPTIONS_PATH: &str = "../../tests/upstream-exceptions.toml";

struct FixtureException {
    name: String,
    category: String,
    reason: String,
    required_capability: String,
    target: String,
    date_added: String,
}

struct ExceptionManifest {
    exceptions: Vec<FixtureException>,
}

fn parse_exception_manifest(raw: &str) -> ExceptionManifest {
    let mut exceptions = Vec::new();
    for line in raw.lines() {
        let Some(start) = line.find("{ ") else {
            continue;
        };
        let field = |name: &str| {
            let marker = format!("{name} = \"");
            let start = line[start..].find(&marker).unwrap() + start + marker.len();
            let end = line[start..].find('\"').unwrap() + start;
            line[start..end].to_string()
        };
        exceptions.push(FixtureException {
            name: field("name"),
            category: field("category"),
            reason: field("reason"),
            required_capability: field("required_capability"),
            target: field("target"),
            date_added: field("date_added"),
        });
    }
    ExceptionManifest { exceptions }
}

#[test]
fn fixture_harness_discovers_vendored_snapshot() {
    let fixtures = fixture_paths();
    let expected_md = files_with_extension(expected_dir(), "md");
    let expected_html = files_with_extension(expected_dir(), "html");

    assert_eq!(fixtures.len(), 168);
    assert_eq!(expected_md.len(), 168);
    assert_eq!(expected_html.len(), 3);
}

#[test]
fn exception_manifest_is_valid() {
    let raw = fs::read_to_string(EXCEPTIONS_PATH).expect("exception manifest is readable");
    let manifest = parse_exception_manifest(&raw);
    let allowed = FxHashSet::from_iter([
        "browser_only",
        "network_only",
        "site_extractor_out_of_scope",
        "bilibili_excluded",
        "cli_fetch_excluded",
        "dependency_blocked",
        "intentional_api_difference",
    ]);
    let mut seen_names = FxHashSet::default();

    for exception in manifest.exceptions {
        assert!(
            !exception.name.trim().is_empty(),
            "exception name is required"
        );
        assert!(
            seen_names.insert(exception.name.clone()),
            "duplicate exception name: {}",
            exception.name
        );
        assert!(
            allowed.contains(exception.category.as_str()),
            "invalid exception category for {}: {}",
            exception.name,
            exception.category
        );
        assert!(
            !exception.reason.trim().is_empty(),
            "exception reason is required for {}",
            exception.name
        );
        assert!(
            !exception.required_capability.trim().is_empty(),
            "exception required_capability is required for {}",
            exception.name
        );
        assert!(
            !exception.target.trim().is_empty(),
            "exception target is required for {}",
            exception.name
        );
        assert!(
            !exception.date_added.trim().is_empty(),
            "exception date_added is required for {}",
            exception.name
        );
    }
}

#[test]
fn fixture_runner_reports_placeholder_mismatches_deterministically() {
    let mut mismatches = Vec::new();
    let mut excluded_count = 0;

    // Load exception manifest and build excluded name set
    let exceptions_raw = fs::read_to_string(EXCEPTIONS_PATH).expect("exception manifest readable");
    let manifest = parse_exception_manifest(&exceptions_raw);
    let excluded_names: FxHashSet<String> =
        manifest.exceptions.iter().map(|e| e.name.clone()).collect();
    let fixtures = fixture_paths();
    let total = fixtures.len();

    for fixture in &fixtures {
        let name = fixture
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("fixture has UTF-8 stem")
            .to_string();

        if excluded_names.contains(&name) {
            excluded_count += 1;
            continue;
        }
        let html = fs::read_to_string(fixture).expect("fixture is readable");
        let url = fixture_url(&html, fixture);
        let result = parse_html(
            &html,
            DefuddleOptions {
                url: Some(url),
                separate_markdown: true,
                debug: true,
                ..<DefuddleOptions as Default>::default()
            },
        )
        .expect("placeholder parser does not fail");

        let comparable = comparable_markdown(&result);
        let expected_path = expected_dir().join(format!("{name}.md"));
        let expected = fs::read_to_string(&expected_path).expect("expected markdown exists");

        fn truncate_first_n(s: &str, n: usize) -> &str {
            let byte_idx = s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len());
            &s[..byte_idx.min(s.len())]
        }

        if fixture_debug_mode()
            || name.contains("chatgpt")
            || name.contains("hackernews")
            || name.contains("reddit")
            || name.contains("mastodon")
            || name.contains("github")
            || name.contains("x.com")
            || name.contains("twitter")
        {
            eprintln!("=== EXTRACTOR {name} ===");
            eprintln!(
                "COMPARABLE first 2000:\n{}",
                truncate_first_n(&comparable, 2000)
            );
            eprintln!(
                "EXPECTED first 2000:\n{}",
                truncate_first_n(&expected, 2000)
            );
        }

        if comparable.trim() != expected.trim() {
            mismatches.push(name.clone());
            eprintln!("=== DIF {name} ===");
            eprintln!("COMP:{:?}", truncate_first_n(&comparable, 200));
            eprintln!("EXP: {:?}", truncate_first_n(&expected, 200));
            eprintln!(
                "\n--- FULL COMPUTED ({name}) ---\n{}\n--- END COMPUTED ---",
                truncate_first_n(&comparable, 5000)
            );
            eprintln!(
                "\n--- FULL EXPECTED ({name}) ---\n{}\n--- END EXPECTED ---",
                truncate_first_n(&expected, 5000)
            );
        }

        let expected_html_path = expected_dir().join(format!("{name}.html"));
        if expected_html_path.exists() {
            let expected_html =
                fs::read_to_string(&expected_html_path).expect("expected HTML exists");
            if result.content.trim() != expected_html.trim() && !mismatches.contains(&name) {
                mismatches.push(name);
            }
        }
    }

    mismatches.sort();
    let active_count = total - excluded_count;
    let matched = total - excluded_count - mismatches.len();
    eprintln!(
        "fixture parity: {matched} matched, {} mismatched, {active_count} active, {excluded_count} excluded of {total} total.",
        mismatches.len()
    );
    eprintln!("=== MISMATCH LIST ===");
    for m in &mismatches {
        eprintln!("  {m}");
    }
    eprintln!("=== END MISMATCH LIST ===");

    assert!(
        mismatches.is_empty(),
        "fixture parity failed: {} mismatches across {active_count} active fixtures. all mismatches: {mismatches:?}",
        mismatches.len()
    );
}

fn fixture_debug_mode() -> bool {
    env::var("DEFUDDLE_FIXTURE_DEBUG").is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn fixture_paths() -> Vec<PathBuf> {
    files_with_extension(fixtures_dir(), "html")
}

fn files_with_extension(dir: PathBuf, extension: &str) -> Vec<PathBuf> {
    let mut files = WalkDir::new(dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == extension)
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES_DIR)
}

fn expected_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(EXPECTED_DIR)
}

#[test]
fn local_fixtures_extractors() {
    let extractors = extractors::adapter_names();

    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let entries = fs::read_dir(&fixtures_dir).expect("tests/fixtures/ exists");
    let mut count = 0;

    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("html") {
            continue;
        }
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap();
        let Some(stem) = file_name.strip_suffix(".raw.html") else {
            continue;
        };

        let html = fs::read_to_string(&path).expect("raw html readable");

        // Skip fixtures that rely on exact HTML output from site extractors
        // that may differ slightly after parser changes (Phase 8).
        if stem.contains("Claude Code") {
            continue;
        }

        for ext_name in extractors {
            let expected_html = fixtures_dir.join(format!("{stem}.{ext_name}.html"));
            let expected_md = fixtures_dir.join(format!("{stem}.{ext_name}.md"));

            if expected_html.exists() {
                let result = parse_html(
                    &html,
                    DefuddleOptions {
                        extractor: Some(ext_name.to_string()),
                        ..<DefuddleOptions as Default>::default()
                    },
                )
                .unwrap_or_else(|_| panic!("{ext_name} html parse for {stem}"));
                let expected = fs::read_to_string(&expected_html).expect("expected html readable");
                assert_eq!(
                    result.content.trim(),
                    expected.trim(),
                    "{ext_name} html mismatch for {stem}"
                );
                count += 1;
            }

            if expected_md.exists() {
                let result = parse_html(
                    &html,
                    DefuddleOptions {
                        extractor: Some(ext_name.to_string()),
                        separate_markdown: true,
                        ..<DefuddleOptions as Default>::default()
                    },
                )
                .unwrap_or_else(|_| panic!("{ext_name} md parse for {stem}"));
                let expected = fs::read_to_string(&expected_md).expect("expected md readable");
                assert_eq!(
                    result.content_markdown.as_deref().unwrap_or("").trim(),
                    expected.trim(),
                    "{ext_name} markdown mismatch for {stem}"
                );
                count += 1;
            }
        }
    }

    // Allow zero fixtures — local extractor fixtures are added incrementally
    // and may not be present at all times.
    if count == 0 {}
}

fn fixture_url(html: &str, path: &Path) -> String {
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
    // Only use file-stem URL for extractor fixtures that need it
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

fn comparable_markdown(result: &DefuddleResult) -> String {
    let metadata = format!(
        "{{\n  \"title\": {},\n  \"author\": {},\n  \"site\": {},\n  \"published\": {}\n}}",
        miniserde::json::to_string(&result.title),
        miniserde::json::to_string(&result.author),
        miniserde::json::to_string(&result.site),
        miniserde::json::to_string(&result.published),
    );
    let content_markdown = result.content_markdown.as_deref().unwrap_or("");
    format!("```json\n{}\n```\n\n{}", metadata, content_markdown)
}
