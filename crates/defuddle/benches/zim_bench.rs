//! Full-archive HTML-to-Markdown workload for the local Wikipedia ZIM snapshot.

use std::{
    collections::BTreeMap,
    env,
    time::{Duration, Instant},
};

use defuddle::{DefuddleOptions, ParseProfile, parse_html, parse_html_profiled};
use divan::{AllocProfiler, Bencher, black_box};
use rayon::prelude::*;
use zim::{DirectoryEntry, MimeType, Zim};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

const DEFAULT_ZIM_PATH: &str = "wikipedia_en_100_nopic_2026-07.zim";
const WIKIPEDIA_URL_PREFIX: &str = "https://en.wikipedia.org/wiki/";

struct ZimPage {
    html: String,
    url: String,
}

struct ZimCorpus {
    pages: Vec<ZimPage>,
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
    before_under_50: usize,
}

fn load_corpus() -> ZimCorpus {
    let path = env::var_os("DEFUDDLE_ZIM").unwrap_or_else(|| DEFAULT_ZIM_PATH.into());
    let zim = Zim::new(&path).unwrap_or_else(|error| {
        panic!(
            "failed to open {}: {error}; set DEFUDDLE_ZIM to the archive path",
            path.to_string_lossy()
        )
    });

    let mut pages = Vec::new();
    let mut html_bytes = 0;
    let mut lossy_pages = 0;

    for item in zim.iterate_by_urls() {
        let entry = item.expect("ZIM directory entry is readable");
        let is_html = matches!(
            &entry.mime_type,
            MimeType::Type(mime) if mime.starts_with("text/html")
        );
        if !is_html {
            continue;
        }

        let content = zim
            .entry_content(&entry)
            .expect("ZIM HTML entry content is readable")
            .expect("ZIM HTML entry has content");
        let bytes = content.to_vec().expect("ZIM HTML content is readable");
        html_bytes += bytes.len();
        let html = String::from_utf8_lossy(&bytes).into_owned();
        if html.as_bytes() != bytes {
            lossy_pages += 1;
        }

        pages.push(ZimPage {
            html,
            url: format!("{WIKIPEDIA_URL_PREFIX}{}", entry.url),
        });
    }

    assert!(!pages.is_empty(), "ZIM archive contains no HTML entries");
    eprintln!(
        "loaded {} HTML pages ({} bytes; {} lossy UTF-8 conversions) from {}",
        pages.len(),
        html_bytes,
        lossy_pages,
        path.to_string_lossy()
    );

    ZimCorpus { pages, html_bytes }
}

fn configured_zim_path() -> std::ffi::OsString {
    env::var_os("DEFUDDLE_ZIM").unwrap_or_else(|| DEFAULT_ZIM_PATH.into())
}

fn configured_batch_size() -> usize {
    env::var("DEFUDDLE_ZIM_BATCH_SIZE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(256)
        .max(1)
}

fn configured_batch_bytes() -> usize {
    env::var("DEFUDDLE_ZIM_BATCH_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(64 * 1024 * 1024)
        .max(1)
}

fn configured_progress_interval() -> Duration {
    Duration::from_secs(
        env::var("DEFUDDLE_ZIM_PROGRESS_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(10),
    )
}

fn load_next_batch<I>(
    zim: &Zim,
    entries: &mut I,
    batch_size: usize,
    batch_bytes: usize,
    scanned_entries: &mut u64,
) -> Option<ZimCorpus>
where
    I: Iterator<Item = zim::Result<DirectoryEntry>>,
{
    let mut pages = Vec::with_capacity(batch_size);
    let mut html_bytes = 0;

    while pages.len() < batch_size && (pages.is_empty() || html_bytes < batch_bytes) {
        let Some(item) = entries.next() else {
            break;
        };
        *scanned_entries += 1;
        let entry = item.expect("ZIM directory entry is readable");
        let is_html = matches!(
            &entry.mime_type,
            MimeType::Type(mime) if mime.starts_with("text/html")
        );
        if !is_html {
            continue;
        }

        let content = zim
            .entry_content(&entry)
            .expect("ZIM HTML entry content is readable")
            .expect("ZIM HTML entry has content");
        let bytes = content.to_vec().expect("ZIM HTML content is readable");
        html_bytes += bytes.len();
        pages.push(ZimPage {
            html: String::from_utf8_lossy(&bytes).into_owned(),
            url: format!("{WIKIPEDIA_URL_PREFIX}{}", entry.url),
        });
    }

    (!pages.is_empty()).then_some(ZimCorpus { pages, html_bytes })
}

fn options_for(page: &ZimPage) -> DefuddleOptions {
    let mut options = DefuddleOptions {
        url: Some(page.url.clone()),
        separate_markdown: true,
        ..Default::default()
    };
    match env::var_os("DEFUDDLE_ZIM_VARIANT").as_deref() {
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

fn report_phase_breakdown(corpus: &ZimCorpus) {
    let started = Instant::now();
    let mut phases = BTreeMap::<&'static str, PhaseAggregate>::new();
    let mut total_phase_nanos = 0;
    let mut retries = 0;
    let mut retry_paths = [0usize; 3];
    let mut retry_aggregates = [RetryAggregate::default(), RetryAggregate::default()];
    let mut markdown_bytes = 0;

    for page in &corpus.pages {
        let (result, profile) = parse_html_profiled(&page.html, options_for(page))
            .expect("profiled benchmark extraction succeeds");
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
            aggregate.before_under_50 += usize::from(before < 50);
        }
        total_phase_nanos += add_profile(&mut phases, &profile);
    }

    let wall_nanos = started.elapsed().as_nanos();
    println!(
        "phase breakdown: {} pages, {} HTML bytes, {} Markdown bytes",
        corpus.pages.len(),
        corpus.html_bytes,
        markdown_bytes
    );
    println!(
        "wall time: {:.3} ms; measured phase time: {:.3} ms; retries: {}",
        nanos_to_ms(wall_nanos),
        nanos_to_ms(total_phase_nanos),
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
            "retry {}: {} calls, {} improved, {} began below 50 words, {:.1} -> {:.1} average words",
            index + 1,
            aggregate.calls,
            aggregate.improved,
            aggregate.before_under_50,
            aggregate.before_words as f64 / aggregate.calls as f64,
            aggregate.after_words as f64 / aggregate.calls as f64
        );
    }
    println!("phase                        total ms   avg/page ms   calls   share");
    for (name, aggregate) in phases {
        let share = aggregate.elapsed_nanos as f64 / total_phase_nanos as f64 * 100.0;
        println!(
            "{name:<28} {:>9.3}   {:>11.3}   {:>5}   {:>5.1}%",
            nanos_to_ms(aggregate.elapsed_nanos),
            nanos_to_ms(aggregate.elapsed_nanos / corpus.pages.len() as u128),
            aggregate.calls,
            share
        );
    }
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

fn nanos_to_ms(nanos: u128) -> f64 {
    nanos as f64 / 1_000_000.0
}

fn benchmark_workers() -> usize {
    let workers = env::var("DEFUDDLE_ZIM_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1);
    workers.max(1)
}

fn process_pages(pages: &[ZimPage]) -> (usize, usize) {
    let mut markdown_bytes = 0;
    for page in pages {
        let result =
            parse_html(&page.html, options_for(page)).expect("benchmark extraction succeeds");
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

    fn process(&self, corpus: &ZimCorpus) -> (usize, usize) {
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

fn run_streaming_throughput() {
    let overall_started = Instant::now();
    let path = configured_zim_path();
    let zim = Zim::new(&path).unwrap_or_else(|error| {
        panic!(
            "failed to open {}: {error}; set DEFUDDLE_ZIM to the archive path",
            path.to_string_lossy()
        )
    });
    let batch_size = configured_batch_size();
    let batch_bytes = configured_batch_bytes();
    let progress_interval = configured_progress_interval();
    let workers = benchmark_workers();
    let processor = PageProcessor::new(workers);
    let mut entries = zim.iterate_by_urls();
    let total_entries = u64::from(zim.header.article_count);
    let mut scanned_entries = 0;
    let mut total_pages = 0;
    let mut total_markdown_bytes = 0;
    let mut total_parse_nanos = 0;
    let mut batches = 0;
    let mut peak_batch_html_bytes = 0;
    let mut last_progress = overall_started;

    eprintln!(
        "streaming start: {} directory entries, {} workers, {} pages/batch, {:.1} MiB/batch",
        total_entries,
        workers,
        batch_size,
        batch_bytes as f64 / 1_048_576.0
    );

    while let Some(batch) = load_next_batch(
        &zim,
        &mut entries,
        batch_size,
        batch_bytes,
        &mut scanned_entries,
    ) {
        peak_batch_html_bytes = peak_batch_html_bytes.max(batch.html_bytes);
        let started = Instant::now();
        let (pages, markdown_bytes) = processor.process(&batch);
        total_parse_nanos += started.elapsed().as_nanos();
        total_pages += pages;
        total_markdown_bytes += markdown_bytes;
        batches += 1;

        if last_progress.elapsed() >= progress_interval {
            report_streaming_progress(
                scanned_entries,
                total_entries,
                total_pages,
                batches,
                overall_started.elapsed(),
            );
            last_progress = Instant::now();
        }
    }

    report_streaming_progress(
        scanned_entries,
        total_entries,
        total_pages,
        batches,
        overall_started.elapsed(),
    );

    println!(
        "streaming throughput: {total_pages} pages, {total_markdown_bytes} Markdown bytes, {workers} workers, {batches} batches, {:.3} ms parser time, {:.3} ms total wall, {:.2} MiB peak batch HTML",
        nanos_to_ms(total_parse_nanos),
        nanos_to_ms(overall_started.elapsed().as_nanos()),
        peak_batch_html_bytes as f64 / 1_048_576.0
    );
}

fn report_streaming_progress(
    scanned_entries: u64,
    total_entries: u64,
    html_pages: usize,
    batches: usize,
    elapsed: Duration,
) {
    let elapsed_secs = elapsed.as_secs_f64();
    let entries_per_second = scanned_entries as f64 / elapsed_secs.max(0.001);
    let remaining_entries = total_entries.saturating_sub(scanned_entries);
    let eta_secs = remaining_entries as f64 / entries_per_second.max(0.001);
    eprintln!(
        "progress: {scanned_entries}/{total_entries} entries ({:.1}%), {html_pages} HTML pages, {batches} batches, {:.0} entries/s, ETA {:.1} min",
        scanned_entries as f64 / total_entries.max(1) as f64 * 100.0,
        entries_per_second,
        eta_secs / 60.0
    );
}

#[divan::bench]
fn wikipedia_zim_markdown(bencher: Bencher) {
    let corpus = load_corpus();
    let workers = benchmark_workers();
    let processor = PageProcessor::new(workers);
    eprintln!("benchmark workers: {workers}");
    bencher.bench_local(|| {
        let (pages, markdown_bytes) = processor.process(&corpus);
        black_box((pages, corpus.html_bytes, markdown_bytes));
    });
}

fn main() {
    if env::var_os("DEFUDDLE_ZIM_PHASES").is_some() {
        report_phase_breakdown(&load_corpus());
        return;
    }
    if env::var_os("DEFUDDLE_ZIM_THROUGHPUT").is_some() {
        run_streaming_throughput();
        return;
    }
    divan::main();
}
