use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rayon::prelude::*;
use zim::{MimeType, Namespace, Target, Zim};

struct HtmlEntry {
    index: u32,
    namespace: Namespace,
    url: String,
    blob: u32,
}

struct ClusterGroup {
    cluster: u32,
    entries: Vec<HtmlEntry>,
}

struct Progress {
    done: AtomicBool,
    groups: AtomicUsize,
    html_entries: AtomicUsize,
    bytes: AtomicUsize,
}

fn configured_path(name: &str, default: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

fn workers() -> usize {
    std::env::var("DEFUDDLE_THREADS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8)
        .max(1)
}

fn output_file_name(entry: &HtmlEntry) -> String {
    let url = entry
        .url
        .strip_prefix('/')
        .unwrap_or(&entry.url)
        .replace(['/', '\\'], "__");
    format!("{:08}_{url}.html", entry.index)
}

fn output_path(root: &Path, entry: &HtmlEntry) -> PathBuf {
    root.join((entry.namespace.as_byte() as char).to_string())
        .join(output_file_name(entry))
}

fn index_html_entries(zim: &Zim) -> Vec<ClusterGroup> {
    let started = Instant::now();
    let mut by_cluster = HashMap::<u32, Vec<HtmlEntry>>::new();
    let mut html_entries = 0;
    let mut last_report = started;

    for index in 0..zim.header.article_count {
        let entry = zim
            .get_by_url_index(index)
            .expect("ZIM directory entry is readable");
        if !matches!(
            &entry.mime_type,
            MimeType::Type(mime) if mime.starts_with("text/html")
        ) {
            continue;
        }
        let Some(Target::Cluster(cluster, blob)) = entry.target else {
            continue;
        };
        by_cluster.entry(cluster).or_default().push(HtmlEntry {
            index,
            namespace: entry.namespace,
            url: entry.url,
            blob,
        });
        html_entries += 1;

        if last_report.elapsed() >= Duration::from_secs(1) {
            eprintln!(
                "index: {index}/{} entries ({:.1}%), {html_entries} HTML entries, {} clusters",
                zim.header.article_count,
                index as f64 / zim.header.article_count as f64 * 100.0,
                by_cluster.len()
            );
            last_report = Instant::now();
        }
    }

    let mut groups = by_cluster
        .into_iter()
        .map(|(cluster, entries)| ClusterGroup { cluster, entries })
        .collect::<Vec<_>>();
    groups.sort_unstable_by_key(|group| group.cluster);
    eprintln!(
        "indexed: {html_entries} HTML entries in {} clusters ({:.3} s)",
        groups.len(),
        started.elapsed().as_secs_f64()
    );
    groups
}

fn write_manifest(groups: &[ClusterGroup], output: &Path) {
    let mut manifest = String::new();
    for group in groups {
        for entry in &group.entries {
            manifest.push(entry.namespace.as_byte() as char);
            manifest.push('\t');
            manifest.push_str(&output_file_name(entry));
            manifest.push('\t');
            manifest.push_str(&entry.url);
            manifest.push('\n');
        }
    }
    fs::write(output.join("manifest.tsv"), manifest).expect("HTML manifest is writable");
}

fn extract_groups(zim: &Zim, groups: &[ClusterGroup], output: &Path, worker_count: usize) {
    let progress = Arc::new(Progress {
        done: AtomicBool::new(false),
        groups: AtomicUsize::new(0),
        html_entries: AtomicUsize::new(0),
        bytes: AtomicUsize::new(0),
    });
    let progress_reporter = Arc::clone(&progress);
    let total_groups = groups.len();
    let total_entries = groups
        .iter()
        .map(|group| group.entries.len())
        .sum::<usize>();
    let reporter = thread::spawn(move || {
        while !progress_reporter.done.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(1));
            eprintln!(
                "extract: {}/{} clusters, {}/{} HTML entries, {:.2} GiB written",
                progress_reporter.groups.load(Ordering::Relaxed),
                total_groups,
                progress_reporter.html_entries.load(Ordering::Relaxed),
                total_entries,
                progress_reporter.bytes.load(Ordering::Relaxed) as f64 / 1_073_741_824.0
            );
        }
    });

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(worker_count)
        .build()
        .expect("extractor worker pool builds");
    pool.install(|| {
        groups.par_iter().for_each(|group| {
            let cluster = zim
                .get_cluster(group.cluster)
                .expect("ZIM cluster is readable");
            let guard = cluster.read().expect("ZIM cluster decompresses");
            for entry in &group.entries {
                let bytes = guard.blob(entry.blob).expect("ZIM HTML blob is readable");
                let path = output_path(output, entry);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).expect("HTML output directory is writable");
                }
                fs::write(path, bytes).expect("HTML output file is writable");
                progress.html_entries.fetch_add(1, Ordering::Relaxed);
                progress.bytes.fetch_add(bytes.len(), Ordering::Relaxed);
            }
            progress.groups.fetch_add(1, Ordering::Relaxed);
        });
    });

    progress.done.store(true, Ordering::Relaxed);
    reporter.join().expect("progress reporter exits");
}

fn main() {
    let archive = configured_path("DEFUDDLE_ZIM", "wikipedia_en_100_nopic_2026-07.zim");
    let output = configured_path("DEFUDDLE_HTML_DIR", "zim-html");
    let worker_count = workers();
    let started = Instant::now();
    let zim = Zim::new(&archive).expect("ZIM archive opens");
    let groups = index_html_entries(&zim);
    let index_done = Instant::now();
    fs::create_dir_all(&output).expect("HTML output directory is writable");
    write_manifest(&groups, &output);
    extract_groups(&zim, &groups, &output, worker_count);
    let finished = Instant::now();
    let html_entries = groups
        .iter()
        .map(|group| group.entries.len())
        .sum::<usize>();

    println!(
        "extracted: {} HTML files, {} clusters, {} workers, output {}, index {:.3} s, extract/write {:.3} s, total {:.3} s",
        html_entries,
        groups.len(),
        worker_count,
        output.display(),
        index_done.duration_since(started).as_secs_f64(),
        finished.duration_since(index_done).as_secs_f64(),
        finished.duration_since(started).as_secs_f64()
    );
}
