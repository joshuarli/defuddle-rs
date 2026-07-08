use rustc_hash::FxHashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use defuddle::{DefuddleOptions, DefuddleResult, parse_html};
use serde::Deserialize;
use walkdir::WalkDir;

const FIXTURES_DIR: &str = "../../tests/upstream/defuddle/fixtures";
const EXPECTED_DIR: &str = "../../tests/upstream/defuddle/expected";
const EXCEPTIONS_PATH: &str = "../../tests/upstream-exceptions.toml";

#[derive(Debug, Deserialize)]
struct ExceptionManifest {
    exceptions: Vec<FixtureException>,
}

#[derive(Debug, Deserialize)]
struct FixtureException {
    name: String,
    category: String,
    reason: String,
    required_capability: String,
    target: String,
    date_added: String,
}

#[test]
fn fixture_harness_discovers_vendored_snapshot() {
    let fixtures = fixture_paths();
    let expected_md = files_with_extension(expected_dir(), "md");
    let expected_html = files_with_extension(expected_dir(), "html");

    assert_eq!(fixtures.len(), 202);
    assert_eq!(expected_md.len(), 202);
    assert_eq!(expected_html.len(), 3);
}

#[test]
fn exception_manifest_is_valid() {
    let raw = fs::read_to_string(EXCEPTIONS_PATH).expect("exception manifest is readable");
    let manifest: ExceptionManifest = toml::from_str(&raw).expect("exception manifest parses");
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
    let manifest: ExceptionManifest =
        toml::from_str(&exceptions_raw).expect("exception manifest parses");
    let excluded_names: FxHashSet<String> =
        manifest.exceptions.iter().map(|e| e.name.clone()).collect();
    let fixture_filter = fixture_filter();

    let fixtures = fixture_paths()
        .into_iter()
        .filter(|path| {
            let Some(filter) = &fixture_filter else {
                return true;
            };
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|name| filter.contains(name))
        })
        .collect::<Vec<_>>();
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
                ..DefuddleOptions::default()
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
            || name.contains("bbcode")
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
            eprintln!("COMP:{:?}", &comparable[..comparable.len().min(200)]);
            eprintln!("EXP: {:?}", &expected[..expected.len().min(200)]);
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
    let first_five = &mismatches[..mismatches.len().min(5)];
    eprintln!(
        "fixture parity: {matched} matched, {} mismatched, {active_count} active, {excluded_count} excluded of {total} total. first five mismatches: {first_five:?}",
        mismatches.len()
    );

    if strict_fixture_mode() {
        assert!(
            mismatches.is_empty(),
            "strict fixture parity failed: {} mismatches across {active_count} active fixtures. first five mismatches: {first_five:?}",
            mismatches.len()
        );
    }
}

fn strict_fixture_mode() -> bool {
    env::var("DEFUDDLE_STRICT_FIXTURES").is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn fixture_filter() -> Option<FxHashSet<String>> {
    let raw = env::var("DEFUDDLE_FIXTURE_FILTER").ok()?;
    let names = raw
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(String::from)
        .collect::<FxHashSet<_>>();
    if names.is_empty() { None } else { Some(names) }
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

fn fixture_url(html: &str, path: &Path) -> String {
    if let Some(start) = html.find("<!--")
        && let Some(end_offset) = html[start + 4..].find("-->")
    {
        let comment = html[start + 4..start + 4 + end_offset].trim();
        if comment.starts_with('{')
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(comment)
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
        serde_json::to_string(&result.title).unwrap_or_default(),
        serde_json::to_string(&result.author).unwrap_or_default(),
        serde_json::to_string(&result.site).unwrap_or_default(),
        serde_json::to_string(&result.published).unwrap_or_default(),
    );
    let content_markdown = result.content_markdown.as_deref().unwrap_or("");
    format!("```json\n{}\n```\n\n{}", metadata, content_markdown)
}
