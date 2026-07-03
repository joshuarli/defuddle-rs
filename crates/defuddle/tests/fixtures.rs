use std::collections::BTreeSet;
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
    let allowed = BTreeSet::from([
        "browser_only",
        "network_only",
        "site_extractor_out_of_scope",
        "bilibili_excluded",
        "cli_fetch_excluded",
        "dependency_blocked",
        "intentional_api_difference",
    ]);

    for exception in manifest.exceptions {
        assert!(
            !exception.name.trim().is_empty(),
            "exception name is required"
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

    for fixture in fixture_paths() {
        let name = fixture
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("fixture has UTF-8 stem")
            .to_string();
        let html = fs::read_to_string(&fixture).expect("fixture is readable");
        let url = fixture_url(&html, &fixture);
        let result = parse_html(
            &html,
            DefuddleOptions {
                url: Some(url),
                separate_markdown: true,
                ..DefuddleOptions::default()
            },
        )
        .expect("placeholder parser does not fail");

        let comparable = comparable_markdown(&result);
        let expected_path = expected_dir().join(format!("{name}.md"));
        let expected = fs::read_to_string(&expected_path).expect("expected markdown exists");
        if comparable.trim() != expected.trim() {
            mismatches.push(name.clone());
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
    assert_eq!(mismatches.len(), 202);
    eprintln!(
        "fixture parity is not enabled yet; placeholder parser mismatched {} fixtures; first five: {:?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(5)]
    );
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
    if let Some(start) = html.find("<!--") {
        if let Some(end_offset) = html[start + 4..].find("-->") {
            let comment = html[start + 4..start + 4 + end_offset].trim();
            if comment.starts_with('{') {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(comment) {
                    if let Some(url) = value.get("url").and_then(|url| url.as_str()) {
                        return url.to_string();
                    }
                }
            }
        }
    }

    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("fixture has UTF-8 stem");
    let url_name = stem.split_once("--").map(|(_, rest)| rest).unwrap_or(stem);
    format!("https://{url_name}")
}

fn comparable_markdown(result: &DefuddleResult) -> String {
    let metadata = serde_json::json!({
        "title": result.title,
        "author": result.author,
        "site": result.site,
        "published": result.published,
    });
    let content_markdown = result.content_markdown.as_deref().unwrap_or("");
    format!(
        "```json\n{}\n```\n\n{}",
        serde_json::to_string_pretty(&metadata).expect("metadata serializes"),
        content_markdown
    )
}
