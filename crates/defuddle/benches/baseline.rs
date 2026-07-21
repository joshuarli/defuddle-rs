use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use defuddle::{DefuddleOptions, DefuddleResult, parse_html};
use miniserde::json::Value;

const ITERATIONS: usize = 100;
const WARMUP_ITERATIONS: usize = 10;

struct Case {
    name: &'static str,
    fixture: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "Hacker News listing",
        fixture: "listing--news.ycombinator.com-news",
    },
    Case {
        name: "Hacker News post",
        fixture: "general--news.ycombinator.com-item-id=12345678",
    },
    Case {
        name: "Hacker News nested comments",
        fixture: "comments--news.ycombinator.com-item-id=12345678",
    },
    Case {
        name: "ordinary article",
        fixture: "general--wikipedia",
    },
    Case {
        name: "extractor-free document",
        fixture: "footnotes--numeric-anchor-id",
    },
];

fn main() {
    println!("defuddle baseline benchmark");
    println!("iterations={ITERATIONS}, warmup={WARMUP_ITERATIONS}");
    println!(
        "case | input bytes | html bytes | markdown bytes | html median | html MiB/s | markdown median | markdown MiB/s"
    );

    let mut checked_html = false;
    for case in CASES {
        let fixture_path = fixture_path(case.fixture);
        let html = fs::read_to_string(&fixture_path).expect("benchmark fixture is readable");
        let url = fixture_url(&html, &fixture_path);
        let expected_md = fs::read_to_string(expected_path(case.fixture, "md"))
            .expect("benchmark Markdown golden is readable");

        let checked = parse(&html, &url, true);
        assert_eq!(
            comparable_markdown(&checked),
            expected_md.trim(),
            "Markdown baseline mismatch for {}",
            case.name
        );

        if let Some(expected_path) = optional_expected_path(case.fixture, "html") {
            let expected_html = fs::read_to_string(expected_path).expect("HTML golden is readable");
            assert_eq!(
                checked.content.trim(),
                expected_html.trim(),
                "HTML baseline mismatch for {}",
                case.name
            );
            checked_html = true;
        }

        let html_timing = measure(&html, &url, false);
        let markdown_timing = measure(&html, &url, true);
        let html_bytes = parse(&html, &url, false).content.len();
        let markdown_bytes = checked
            .content_markdown
            .as_deref()
            .unwrap_or_default()
            .len();

        println!(
            "{} | {} | {} | {} | {} | {:.2} | {} | {:.2}",
            case.name,
            html.len(),
            html_bytes,
            markdown_bytes,
            format_duration(html_timing.median),
            throughput_mib_per_second(html.len(), html_timing.median),
            format_duration(markdown_timing.median),
            throughput_mib_per_second(html.len(), markdown_timing.median),
        );
    }
    assert!(checked_html, "the baseline must exercise an HTML golden");
}

struct Timing {
    median: Duration,
}

fn measure(html: &str, url: &str, markdown: bool) -> Timing {
    for _ in 0..WARMUP_ITERATIONS {
        black_box(parse(html, url, markdown));
    }
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        black_box(parse(html, url, markdown));
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    Timing {
        median: samples[samples.len() / 2],
    }
}

fn parse(html: &str, url: &str, markdown: bool) -> DefuddleResult {
    parse_html(
        html,
        DefuddleOptions {
            url: Some(url.to_string()),
            separate_markdown: markdown,
            ..Default::default()
        },
    )
    .expect("benchmark extraction succeeds")
}

fn comparable_markdown(result: &DefuddleResult) -> String {
    let metadata = format!(
        "{{\n  \"title\": {},\n  \"author\": {},\n  \"site\": {},\n  \"published\": {}\n}}",
        miniserde::json::to_string(&result.title),
        miniserde::json::to_string(&result.author),
        miniserde::json::to_string(&result.site),
        miniserde::json::to_string(&result.published),
    );
    format!(
        "```json\n{metadata}\n```\n\n{}",
        result.content_markdown.as_deref().unwrap_or_default()
    )
}

fn fixture_path(stem: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/upstream/defuddle/fixtures")
        .join(format!("{stem}.html"))
}

fn expected_path(stem: &str, extension: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/upstream/defuddle/expected")
        .join(format!("{stem}.{extension}"))
}

fn optional_expected_path(stem: &str, extension: &str) -> Option<PathBuf> {
    let path = expected_path(stem, extension);
    path.exists().then_some(path)
}

fn fixture_url(html: &str, path: &Path) -> String {
    let Some(start) = html.find("<!--") else {
        return fallback_url(path);
    };
    let Some(end) = html[start + 4..].find("-->") else {
        return fallback_url(path);
    };
    let comment = html[start + 4..start + 4 + end].trim();
    if let Ok(Value::Object(object)) = miniserde::json::from_str(comment)
        && let Some(Value::String(url)) = object.get("url")
    {
        return url.to_string();
    }
    fallback_url(path)
}

fn fallback_url(path: &Path) -> String {
    let stem = path.file_stem().and_then(|value| value.to_str()).unwrap();
    if stem.contains("wikipedia") {
        "https://wikipedia".to_string()
    } else {
        String::new()
    }
}

fn throughput_mib_per_second(bytes: usize, duration: Duration) -> f64 {
    bytes as f64 / (1024.0 * 1024.0) / duration.as_secs_f64()
}

fn format_duration(duration: Duration) -> String {
    if duration.as_micros() < 1_000 {
        format!("{}us", duration.as_micros())
    } else {
        format!("{:.2}ms", duration.as_secs_f64() * 1_000.0)
    }
}
