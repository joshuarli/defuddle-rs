//! End-to-end workloads for the public HTML extraction path.

use defuddle::{DefuddleOptions, parse_html};
use divan::{AllocProfiler, Bencher, black_box};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

const SUBSTACK_URL: &str =
    "https://semianalysis.substack.com/p/claude-code-is-the-inflection-point";
const WIKIPEDIA_URL: &str = "https://wikipedia";
const GITHUB_URL: &str = "https://github.com/kepano/defuddle/issues/56";
const HACKERNEWS_URL: &str = "https://news.ycombinator.com/item?id=12345678";

const SUBSTACK_RAW: &str =
    include_str!("../tests/fixtures/Claude Code is the Inflection Point.raw.html");
const SUBSTACK_BODY: &str =
    include_str!("../tests/fixtures/Claude Code is the Inflection Point.substack.html");
const WIKIPEDIA: &str =
    include_str!("../../../tests/upstream/defuddle/fixtures/general--wikipedia.html");
const GITHUB_ISSUE: &str =
    include_str!("../../../tests/upstream/defuddle/fixtures/general--github.com-issue-56.html");
const HACKERNEWS_COMMENTS: &str = include_str!(
    "../../../tests/upstream/defuddle/fixtures/comments--news.ycombinator.com-item-id=12345678.html"
);
const FOOTNOTES: &str =
    include_str!("../../../tests/upstream/defuddle/fixtures/footnotes--wikipedia-references.html");

fn bench_parse(bencher: Bencher, html: &'static str, url: &'static str, markdown: bool) {
    bencher.bench_local(|| {
        let result = parse_html(
            html,
            DefuddleOptions {
                url: Some(url.to_string()),
                separate_markdown: markdown,
                ..Default::default()
            },
        )
        .expect("benchmark extraction succeeds");
        black_box(result);
    });
}

#[divan::bench]
fn substack_raw_html(bencher: Bencher) {
    bench_parse(bencher, SUBSTACK_RAW, SUBSTACK_URL, false);
}

#[divan::bench]
fn substack_raw_markdown(bencher: Bencher) {
    bench_parse(bencher, SUBSTACK_RAW, SUBSTACK_URL, true);
}

#[divan::bench]
fn substack_body_html(bencher: Bencher) {
    bench_parse(bencher, SUBSTACK_BODY, SUBSTACK_URL, false);
}

#[divan::bench]
fn substack_body_markdown(bencher: Bencher) {
    bench_parse(bencher, SUBSTACK_BODY, SUBSTACK_URL, true);
}

#[divan::bench]
fn wikipedia_html(bencher: Bencher) {
    bench_parse(bencher, WIKIPEDIA, WIKIPEDIA_URL, false);
}

#[divan::bench]
fn wikipedia_markdown(bencher: Bencher) {
    bench_parse(bencher, WIKIPEDIA, WIKIPEDIA_URL, true);
}

#[divan::bench]
fn github_issue_html(bencher: Bencher) {
    bench_parse(bencher, GITHUB_ISSUE, GITHUB_URL, false);
}

#[divan::bench]
fn github_issue_markdown(bencher: Bencher) {
    bench_parse(bencher, GITHUB_ISSUE, GITHUB_URL, true);
}

#[divan::bench]
fn hackernews_comments_html(bencher: Bencher) {
    bench_parse(bencher, HACKERNEWS_COMMENTS, HACKERNEWS_URL, false);
}

#[divan::bench]
fn hackernews_comments_markdown(bencher: Bencher) {
    bench_parse(bencher, HACKERNEWS_COMMENTS, HACKERNEWS_URL, true);
}

#[divan::bench]
fn wikipedia_footnotes_html(bencher: Bencher) {
    bench_parse(bencher, FOOTNOTES, WIKIPEDIA_URL, false);
}

#[divan::bench]
fn wikipedia_footnotes_markdown(bencher: Bencher) {
    bench_parse(bencher, FOOTNOTES, WIKIPEDIA_URL, true);
}

fn main() {
    divan::main();
}
