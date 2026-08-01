//! Full-archive HTML-to-Markdown workload for the local Wikipedia ZIM snapshot.

use std::{collections::BTreeMap, env, time::Instant};

use defuddle::{DefuddleOptions, ParseProfile, parse_html, parse_html_profiled};
use divan::{AllocProfiler, Bencher, black_box};
use zim::{MimeType, Zim};

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

#[divan::bench]
fn wikipedia_zim_markdown(bencher: Bencher) {
    let corpus = load_corpus();
    bencher.bench_local(|| {
        let mut markdown_bytes = 0;
        for page in &corpus.pages {
            let result =
                parse_html(&page.html, options_for(page)).expect("benchmark extraction succeeds");
            markdown_bytes += result.content_markdown.as_deref().map_or(0, str::len);
            black_box(result);
        }
        black_box((corpus.pages.len(), corpus.html_bytes, markdown_bytes));
    });
}

fn main() {
    if env::var_os("DEFUDDLE_ZIM_PHASES").is_some() {
        report_phase_breakdown(&load_corpus());
        return;
    }
    divan::main();
}
