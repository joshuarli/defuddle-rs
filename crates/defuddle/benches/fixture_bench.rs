use std::{collections::BTreeMap, fs, path::PathBuf, time::Instant};

use defuddle::{DefuddleOptions, ParseProfile, parse_html, parse_html_profiled};
use rustybench::{AllocProfiler, Bencher, black_box};

#[path = "../tests/common/fixture_url.rs"]
mod fixture_url;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

struct HtmlPage {
    html: String,
    url: String,
}

struct Corpus {
    pages: Vec<HtmlPage>,
    html_bytes: usize,
}

fn load_corpus() -> Corpus {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/upstream/defuddle/fixtures");
    let mut paths = fs::read_dir(fixtures)
        .expect("upstream fixtures directory is readable")
        .map(|entry| entry.expect("fixture directory entry is readable").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "html"))
        .collect::<Vec<_>>();
    paths.sort_unstable();

    let mut html_bytes = 0;
    let pages = paths
        .into_iter()
        .map(|path| {
            let html = fs::read_to_string(&path).expect("fixture HTML is readable");
            html_bytes += html.len();
            let url = fixture_url::fixture_url(&html, &path);
            HtmlPage { html, url }
        })
        .collect::<Vec<_>>();

    assert!(!pages.is_empty(), "upstream fixture corpus is empty");
    eprintln!(
        "loaded {} upstream HTML fixtures ({} bytes)",
        pages.len(),
        html_bytes
    );
    Corpus { pages, html_bytes }
}

fn options_for(page: &HtmlPage, separate_markdown: bool) -> DefuddleOptions {
    DefuddleOptions {
        url: Some(page.url.clone()),
        separate_markdown,
        ..Default::default()
    }
}

fn process_corpus(corpus: &Corpus, separate_markdown: bool) {
    let mut output_html_bytes = 0;
    let mut output_markdown_bytes = 0;

    for page in &corpus.pages {
        let result = parse_html(&page.html, options_for(page, separate_markdown))
            .expect("fixture parse succeeds");
        output_html_bytes += result.content.len();
        output_markdown_bytes += result
            .content_markdown
            .as_deref()
            .map_or(0, str::len);
        black_box(result);
    }

    black_box((
        corpus.pages.len(),
        corpus.html_bytes,
        output_html_bytes,
        output_markdown_bytes,
    ));
}

#[derive(Default)]
struct PhaseTotal {
    calls: usize,
    elapsed_nanos: u128,
}

fn add_profile(phases: &mut BTreeMap<&'static str, PhaseTotal>, profile: &ParseProfile) {
    for phase in &profile.phases {
        let total = phases.entry(phase.name).or_default();
        total.calls += 1;
        total.elapsed_nanos += phase.elapsed_nanos;
    }
}

fn report_phases(corpus: &Corpus) {
    let started = Instant::now();
    let mut phases = BTreeMap::<&'static str, PhaseTotal>::new();
    let mut output_markdown_bytes = 0;

    for page in &corpus.pages {
        let (result, profile) = parse_html_profiled(&page.html, options_for(page, true))
            .expect("profiled fixture parse succeeds");
        output_markdown_bytes += result.content_markdown.as_deref().map_or(0, str::len);
        add_profile(&mut phases, &profile);
        black_box(result);
    }

    let total_nanos = phases.values().map(|phase| phase.elapsed_nanos).sum::<u128>();
    println!(
        "phase pass: {} fixtures, {} HTML bytes, {} Markdown bytes",
        corpus.pages.len(),
        corpus.html_bytes,
        output_markdown_bytes
    );
    println!("wall time: {:.3} ms", started.elapsed().as_secs_f64() * 1000.0);
    println!("phase                        total ms   avg/page ms   calls   share");
    for (name, total) in phases {
        println!(
            "{name:<28} {:>9.3}   {:>11.3}   {:>5}   {:>5.1}%",
            total.elapsed_nanos as f64 / 1_000_000.0,
            total.elapsed_nanos as f64 / corpus.pages.len() as f64 / 1_000_000.0,
            total.calls,
            total.elapsed_nanos as f64 / total_nanos as f64 * 100.0
        );
    }
}

#[rustybench::bench]
fn upstream_fixtures_html(bencher: Bencher) {
    let corpus = load_corpus();
    bencher.bench_local(|| process_corpus(&corpus, false));
}

#[rustybench::bench]
fn upstream_fixtures_markdown(bencher: Bencher) {
    let corpus = load_corpus();
    bencher.bench_local(|| process_corpus(&corpus, true));
}

fn main() {
    if std::env::var_os("DEFUDDLE_PHASES").is_some() {
        report_phases(&load_corpus());
        return;
    }
    rustybench::main();
}
