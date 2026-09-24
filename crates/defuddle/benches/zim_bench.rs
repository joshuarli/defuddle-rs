//! Full-corpus HTML-to-Markdown workload for an extracted Wikipedia directory.

use std::{collections::BTreeMap, env, fs, time::Instant};

use defuddle::{DefuddleOptions, ParseProfile, parse_html, parse_html_profiled};
use rustybench::{AllocProfiler, Bencher, black_box};
use rayon::prelude::*;
use walkdir::WalkDir;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

const WIKIPEDIA_URL_PREFIX: &str = "https://en.wikipedia.org/wiki/";

struct HtmlPage {
    html: String,
    url: String,
}

struct HtmlCorpus {
    pages: Vec<HtmlPage>,
    html_bytes: usize,
}

#[derive(Default)]
struct PhaseAggregate {
    calls: usize,
    elapsed_nanos: u128,
}

#[derive(Default)]
struct RetryAggregate {
    calls: usize,
    improved: usize,
    before_words: usize,
    after_words: usize,
}

fn html_directory() -> std::path::PathBuf {
    env::var_os("DEFUDDLE_HTML_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("set DEFUDDLE_HTML_DIR to an extracted HTML directory"))
}

fn load_corpus() -> HtmlCorpus {
    let directory = html_directory();
    let mut urls = std::collections::HashMap::new();
    let manifest = directory.join("manifest.tsv");
    if let Ok(raw) = fs::read_to_string(&manifest) {
        for line in raw.lines() {
            let mut fields = line.splitn(3, '\t');
            let Some(namespace) = fields.next() else {
                continue;
            };
            let Some(file_name) = fields.next() else {
                continue;
            };
            let Some(url) = fields.next() else {
                continue;
            };
            urls.insert(
                format!("{namespace}/{file_name}"),
                format!("{WIKIPEDIA_URL_PREFIX}{url}"),
            );
        }
    }

    let mut pages = Vec::new();
    let mut html_bytes = 0;
    for entry in WalkDir::new(&directory)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "html")
        })
    {
        let html = fs::read_to_string(entry.path()).expect("extracted HTML is readable");
        html_bytes += html.len();
        let relative = entry
            .path()
            .strip_prefix(&directory)
            .expect("HTML path is under directory")
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        let url = urls
            .get(&relative)
            .cloned()
            .unwrap_or_else(|| format!("{WIKIPEDIA_URL_PREFIX}{relative}"));
        pages.push(HtmlPage { html, url });
    }
    pages.sort_unstable_by(|left, right| left.url.cmp(&right.url));
    assert!(!pages.is_empty(), "HTML directory contains no HTML files");
    eprintln!(
        "loaded {} extracted HTML pages ({} bytes) from {}",
        pages.len(),
        html_bytes,
        directory.display()
    );
    HtmlCorpus { pages, html_bytes }
}

fn options_for(page: &HtmlPage) -> DefuddleOptions {
    let mut options = DefuddleOptions {
        url: Some(page.url.clone()),
        separate_markdown: true,
        ..Default::default()
    };
    match env::var_os("DEFUDDLE_VARIANT").as_deref() {
        Some(value) if value == "no-selector-removal" => {
            options.remove_exact_selectors = Some(false);
            options.remove_partial_selectors = Some(false);
        }
        Some(value) if value == "no-exact-selectors" => {
            options.remove_exact_selectors = Some(false);
        }
        Some(value) if value == "no-partial-selectors" => {
            options.remove_partial_selectors = Some(false);
        }
        Some(value) if value == "no-scoring" => options.remove_low_scoring = Some(false),
        Some(value) if value == "no-content-patterns" => {
            options.remove_content_patterns = Some(false);
        }
        Some(value) if value == "no-standardize" => options.standardize = Some(false),
        Some(value) if value == "html-only" => options.separate_markdown = false,
        _ => {}
    }
    options
}

fn process_pages(pages: &[HtmlPage]) -> (usize, usize) {
    let mut markdown_bytes = 0;
    for page in pages {
        let result = parse_html(&page.html, options_for(page)).expect("benchmark parse succeeds");
        markdown_bytes += result.content_markdown.as_deref().map_or(0, str::len);
        black_box(result);
    }
    (pages.len(), markdown_bytes)
}

struct PageProcessor {
    workers: usize,
    pool: Option<rayon::ThreadPool>,
}

impl PageProcessor {
    fn new(workers: usize) -> Self {
        let pool = (workers > 1).then(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .expect("benchmark worker pool builds")
        });
        Self { workers, pool }
    }

    fn process(&self, corpus: &HtmlCorpus) -> (usize, usize) {
        if self.workers == 1 {
            return process_pages(&corpus.pages);
        }
        let markdown_bytes = self
            .pool
            .as_ref()
            .expect("parallel benchmark pool exists")
            .install(|| {
                corpus
                    .pages
                    .par_iter()
                    .map(|page| process_pages(std::slice::from_ref(page)).1)
                    .sum()
            });
        (corpus.pages.len(), markdown_bytes)
    }
}

fn benchmark_workers() -> usize {
    env::var("DEFUDDLE_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1)
}

fn add_profile(
    phases: &mut BTreeMap<&'static str, PhaseAggregate>,
    profile: &ParseProfile,
) -> u128 {
    for phase in &profile.phases {
        let aggregate = phases.entry(phase.name).or_default();
        aggregate.calls += 1;
        aggregate.elapsed_nanos += phase.elapsed_nanos;
    }
    profile.total_nanos()
}

fn report_phases(corpus: &HtmlCorpus) {
    let started = Instant::now();
    let mut phases = BTreeMap::<&'static str, PhaseAggregate>::new();
    let mut retries = 0;
    let mut retry_paths = [0usize; 3];
    let mut retry_aggregates = [RetryAggregate::default(), RetryAggregate::default()];
    let mut markdown_bytes = 0;

    for page in &corpus.pages {
        let (result, profile) = parse_html_profiled(&page.html, options_for(page))
            .expect("profiled benchmark parse succeeds");
        markdown_bytes += result.content_markdown.as_deref().map_or(0, str::len);
        retries += profile.retry_count;
        retry_paths[profile.retry_word_counts.len().min(2)] += 1;
        for (index, &(before, after)) in profile.retry_word_counts.iter().enumerate() {
            let Some(aggregate) = retry_aggregates.get_mut(index) else {
                continue;
            };
            aggregate.calls += 1;
            aggregate.improved += usize::from(after > before);
            aggregate.before_words += before;
            aggregate.after_words += after;
        }
        add_profile(&mut phases, &profile);
    }

    let total_phase_nanos: u128 = phases.values().map(|phase| phase.elapsed_nanos).sum();
    println!(
        "phase breakdown: {} pages, {} HTML bytes, {} Markdown bytes",
        corpus.pages.len(),
        corpus.html_bytes,
        markdown_bytes
    );
    println!(
        "wall time: {:.3} ms; measured phase time: {:.3} ms; retries: {}",
        started.elapsed().as_secs_f64() * 1000.0,
        total_phase_nanos as f64 / 1_000_000.0,
        retries
    );
    println!(
        "retry paths: {} without retry, {} with one retry, {} with two retries",
        retry_paths[0], retry_paths[1], retry_paths[2]
    );
    for (index, aggregate) in retry_aggregates.iter().enumerate() {
        if aggregate.calls == 0 {
            continue;
        }
        println!(
            "retry {}: {} calls, {} improved, {:.1} -> {:.1} average words",
            index + 1,
            aggregate.calls,
            aggregate.improved,
            aggregate.before_words as f64 / aggregate.calls as f64,
            aggregate.after_words as f64 / aggregate.calls as f64
        );
    }
    println!("phase                        total ms   avg/page ms   calls   share");
    for (name, aggregate) in phases {
        println!(
            "{name:<28} {:>9.3}   {:>11.3}   {:>5}   {:>5.1}%",
            aggregate.elapsed_nanos as f64 / 1_000_000.0,
            aggregate.elapsed_nanos as f64 / corpus.pages.len() as f64 / 1_000_000.0,
            aggregate.calls,
            aggregate.elapsed_nanos as f64 / total_phase_nanos as f64 * 100.0
        );
    }
}

#[rustybench::bench]
fn html_directory_markdown(bencher: Bencher) {
    let corpus = load_corpus();
    let processor = PageProcessor::new(benchmark_workers());
    bencher.bench_local(|| {
        let result = processor.process(&corpus);
        black_box((result, corpus.html_bytes));
    });
}

fn main() {
    if env::var_os("DEFUDDLE_PHASES").is_some() {
        report_phases(&load_corpus());
        return;
    }
    rustybench::main();
}
