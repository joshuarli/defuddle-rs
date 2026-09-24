# Performance

Defuddle performance work is measured through the public `parse_html()` path
over an extracted Wikipedia HTML directory. The benchmark binary uses `rustybench` and the
system allocator wrapped by `AllocProfiler`; it is intentionally useful as a
future PGO workload, but PGO is not currently part of the build or release
workflow.

## Core workflow

Run the directory benchmark with:

```bash
make bench
```

The `extract-zim-html` target runs `crates/defuddle/benches/zim_extract.rs`.
It indexes HTML entries by cluster, decompresses each cluster once, writes the
HTML files in parallel, and emits `manifest.tsv` for URL preservation. The
`bench` target runs `crates/defuddle/benches/zim_bench.rs`; it requires the
resulting directory and loads all HTML before `rustybench` starts timing:

```bash
DEFUDDLE_ZIM=/path/to/archive.zim \
DEFUDDLE_HTML_DIR=/path/to/extracted-html \
make extract-zim-html
DEFUDDLE_HTML_DIR=/path/to/extracted-html make bench
DEFUDDLE_HTML_DIR=/path/to/extracted-html DEFUDDLE_PHASES=1 make bench
```

The small reference archive produces 1,314 HTML files totaling about 64 MiB
on disk. The directory benchmark times only repeated `parse_html()` calls with
`separate_markdown: true`; ZIM access and extraction are deliberately outside
the timed operation. Set `DEFUDDLE_PHASES=1` to run one unbenchmarked pass and
print aggregate wall time, total and average phase time, invocation counts,
phase shares, and retry counts across the extracted directory.

For controlled phase isolation, set `DEFUDDLE_VARIANT` to one of
`no-selector-removal`, `no-exact-selectors`, `no-partial-selectors`,
`no-scoring`, `no-content-patterns`, `no-standardize`, or `html-only`.

`DEFUDDLE_THREADS` controls the parser worker count. Use the directory
benchmark for throughput comparisons; use the extractor's cluster progress
output separately when measuring ZIM extraction.

```bash
DEFUDDLE_HTML_DIR=/path/to/extracted-html DEFUDDLE_THREADS=8 make bench
```

The fixture harness also includes 50 deterministic article pages selected from
the substantial ZIM payloads under
`tests/upstream/defuddle/fixtures/wikipedia-zim--*.html`, with matching Markdown
goldens under `tests/upstream/defuddle/expected/wikipedia-zim--*.md`. These
fixtures are intended as an accuracy gate before optimizing the full-corpus
workload; the archive itself remains an external benchmark input.

When the ZIM archive is unavailable, build the fixed 50-page corpus from those
fixtures. The preparation script copies the HTML and writes a URL manifest in
the same layout expected by `zim_bench`:

```bash
python3 benchmarks/prepare_wikipedia_fixtures.py /tmp/defuddle-wikipedia-50
DEFUDDLE_HTML_DIR=/tmp/defuddle-wikipedia-50 make bench
```

Use a new empty output directory for each preparation. This smaller corpus is
the local baseline below; it is not interchangeable with the 1,314-page ZIM
corpus.

For repeatable comparisons, keep the source revision, Rust toolchain, machine,
allocator, corpus, worker count, and benchmark options constant. Capture both
revisions with the same sample count. The local `../rustybench` checkout is the
benchmark runner and a path development dependency; keep its revision constant
too. For a quick single run:

```bash
DEFUDDLE_HTML_DIR=/path/to/extracted-html \
  cargo bench -p defuddle --bench zim_bench -- \
  --sample-count 5 --sample-size 1
```

To save a repeatable JSON baseline, use the `rustybench baseline` command. It
warms up once and records three measured runs by default:

```bash
DEFUDDLE_HTML_DIR=/path/to/extracted-html \
  cargo run --manifest-path ../rustybench/Cargo.toml -- baseline \
  --root . --baseline benchmarks/candidate.json \
  --sample-count 5 --sample-size 1 -- \
  cargo bench -p defuddle --bench zim_bench
cargo run --manifest-path ../rustybench/Cargo.toml -- \
  diff benchmarks/m1-wikipedia-50-2026-09-23.json benchmarks/candidate.json
```

For phase-level Markdown timing, set `DEFUDDLE_PROFILE=1`. This prints the
existing parse phases plus `markdown preprocess`, `markdown convert`, and
`markdown postprocess` without changing normal output:

```bash
DEFUDDLE_PROFILE=1 cargo run --release -p defuddle-cli -- --markdown \
  --url https://example.com/article input.html > /tmp/article.md
```

Compare medians across several runs rather than relying on a single fastest or
slowest sample. A change that is not clearly outside normal run-to-run noise
should not be kept merely because one run improved.

## Optimization scope rules

Benchmark fixtures are measurement inputs, not optimization targets. Do not
add page-, domain-, extractor-, fixture-, or workload-specific fast paths to
improve a named benchmark. A retained optimization must improve a generic
class of inputs through the public `parse_html()` pipeline and must preserve
the same behavior contract for inputs outside the benchmark set. If a result
only helps one capture or one site shape, treat it as a fixture-specific
workaround and reject it from the performance architecture.

The benchmark suite may be used to discover bottlenecks and validate broad
regressions, but not to justify special cases keyed by URL, extractor name,
fixture identity, document dimensions, or incidental markup fingerprints.

## Reading allocation output

`rustybench` reports two useful allocation views for each timed operation:

- `max_alloc_bytes` is the high-water mark of live allocations during one operation.
  It is a peak live-allocation measurement, not process RSS.
- `alloc_count` and `alloc_bytes` are cumulative allocations over the operation.
  They include allocations that have already been freed and are useful for
  finding churn.

Use `/usr/bin/time -l` or a platform profiler when process RSS or the total
process footprint is needed; those measurements include the runtime and
benchmark harness.

## Current local baseline and goals

The saved baseline is
[`benchmarks/m1-wikipedia-50-2026-09-23.json`](benchmarks/m1-wikipedia-50-2026-09-23.json).
It was captured on 2026-09-23 from core source revision `e854032` after
switching the benchmark harness to `rustybench` revision `56a0611`. The host
was an Apple M1 with 8 GiB RAM, macOS 27.0, and
`rustc 1.100.0-nightly (574ff7d98 2026-09-14)`. The fixed corpus contains 50
vendored Wikipedia pages, 34,877,998 HTML bytes, and 7,607,903 Markdown
output bytes. `DEFUDDLE_THREADS` was unset, so parsing used one worker.

The baseline command used one warmup, three measured runs, five samples per
run, and one complete 50-page pass per sample:

```bash
DEFUDDLE_HTML_DIR=/path/to/prepared-wikipedia-50 \
  cargo run --manifest-path ../rustybench/Cargo.toml -- baseline \
  --root . --baseline benchmarks/m1-wikipedia-50-2026-09-23.json \
  --warmup 1 --runs 3 --sample-count 5 --sample-size 1 -- \
  cargo bench -p defuddle --bench zim_bench
```

| Metric per 50-page pass | Baseline | Goal |
| --- | ---: | ---: |
| Median wall time | 12.43 s | **≤ 9.31 s** (at least 25% lower) |
| Cumulative allocated bytes | 3,989,210,858 | **≤ 3,191,368,686** (at least 20% lower) |
| Cumulative allocation count | 21,553,106 | **≤ 22,630,761** (at most 5% higher) |
| Peak live allocated bytes | 16,270,796 | **≤ 17,084,335** (at most 5% higher) |

The three run medians were 12.20, 12.62, and 12.43 seconds. Treat an apparent
gain within this run-to-run spread as inconclusive and repeat the measurement.
The stretch goal is **≤ 8.70 seconds** for the same pass, about 30% below this
baseline. The primary wall-time and allocation-byte goals must both be met;
the stretch goal is additional.

The opt-in phase pass on this corpus measured 12.48 seconds wall time and no
retries. Its largest measured phases were selector removal (5.66 seconds,
46.9% of phase time), generic scoring (1.74 seconds, 14.4%), standardization
(1.53 seconds, 12.7%), and content patterns (1.32 seconds, 11.0%). Phase
timings are diagnostic and are not interchangeable with the benchmark median.

To claim the goals, rebuild the same 50-page corpus, use the same `rustybench`
revision and sample settings, and save a new JSON result. Compare its median,
allocation bytes, and peak live allocation to the saved baseline. Run the
fixture harness and `cargo test --workspace` to establish unchanged extraction
behavior. When a full ZIM corpus is available, capture a separate baseline and
confirm that the optimization helps there too before treating it as a broad
throughput win. Do not substitute a full-corpus result for this 50-page goal.

## 2026-09-23 goal result

The goal is met on the fixed 50-page corpus. The matching one-warmup,
three-measured-run result is saved in
[`benchmarks/m1-wikipedia-50-goal-2026-09-23.json`](benchmarks/m1-wikipedia-50-goal-2026-09-23.json).
The three run medians were 7.279, 7.278, and 7.290 seconds.

| Metric per 50-page pass | Baseline | Result | Change |
| --- | ---: | ---: | ---: |
| Median wall time | 12.426 s | 7.279 s | **-41.42%** |
| Cumulative allocated bytes | 3,989,210,858 | 2,907,093,403 | **-27.13%** |
| Cumulative allocation count | 21,553,106 | 19,606,268 | -9.03% |
| Peak live allocated bytes | 16,270,796 | 16,270,796 | unchanged |

Two separate optimizations produced the result:

- `cc99f80 Avoid selector parsing in code ancestor guard` checks ancestor tag
  names directly during selector cleanup. A one-pass check after this change
  measured 11.82 seconds and 2.91 GB allocated. The fixture harness passed.
- `fee5c2f Index partial cleanup patterns by prefix` groups the 527 static
  clutter patterns by their first two bytes and scans each attribute value
  once. A differential test checks the indexed matcher against the old
  substring, exact-ID, logo, and hyphen-boundary rules. The fixture harness
  passed with unchanged expected output.

The opt-in phase pass moved selector removal from 5.66 to 0.75 seconds across
the same pages, with zero retries in both runs. The full-corpus ZIM archive was
not available on this machine, so this result establishes the documented
50-page goal rather than a full-corpus throughput claim.

## Historical fixture benchmark results

These are older representative medians from the former Divan fixture suite,
with 100 samples on the development machine. They are historical context, not
the baseline or goal for the current directory benchmark.

| Workload | Median | Peak live allocation |
| --- | ---: | ---: |
| `substack_body_html` | 2.39 ms | 520 KB |
| `substack_body_markdown` | 13.2 ms | 727 KB |
| `substack_raw_html` | 4.66 ms | 1.01 MB |
| `substack_raw_markdown` | 16.2 ms | 1.01 MB |
| `wikipedia_html` | 17.2 ms | 1.01 MB |
| `wikipedia_markdown` | 20.6 ms | 1.18 MB |
| `github_issue_markdown` | 4.05 ms | 532 KB |
| `hackernews_comments_markdown` | 702 µs | 124 KB |

The remaining dominant wall-time path is Markdown postprocessing. A release
profile of the Substack body fixture measured approximately 8.2 ms in
`markdown preprocess`, 0.9 ms in `markdown convert`, and 43.0 ms in
`markdown postprocess` before the latest optimization. Generic extraction is
dominated by repeated DOM selection and cleanup passes; direct subtree
traversal and batched cleanup selectors now remove the most expensive repeated
scans. Peak memory is highest when Markdown conversion adds its parser and
intermediate strings.

## Optimization log

Each accepted optimization is kept in its own commit so that its benchmark
effect and behavior change remain easy to bisect.

- `407a056 Optimize Markdown preprocessing`: skip the second Defuddle DOM parse
  for fragments without images, tables, or iframe embeds. Wall time was within
  noise, but the low-complexity path reduced cumulative allocation on ordinary
  fragments.
- `86c0884 Optimize subtree selector traversal`: walk a selected subtree
  directly instead of scanning every live node and checking containment. On
  `substack_body_html`, the controlled old/new comparison moved from 29.5 ms to
  2.46 ms median with unchanged allocation counters.
- `79c59b2 Release removed DOM node payloads`: replace removed node payloads with
  a marker and release their strings, attributes, and child vectors immediately.
  `wikipedia_html` peak live allocation fell from 1.070 MB to 1.013 MB.
- `f423dec Release source DOM after site extraction`: release the original page
  DOM as soon as a site adapter has built its output DOM. `substack_raw_markdown`
  peak live allocation fell from 2.364 MB to 1.645 MB with unchanged cumulative
  allocation totals.
- `589eefb Avoid duplicate extraction retry`: remove the final retry in
  `parse_internal()` because it used exactly the same removal configuration as
  the preceding retry. This saves a parse for short pages without changing the
  selected result.
- `563ee40 Instrument Markdown conversion phases`: add opt-in
  `DEFUDDLE_PROFILE=1` timing for Markdown serialization, preprocessing,
  conversion, and postprocessing.
- `d8f27f6 Skip Markdown spacing scans for plain text`: avoid the 29-regex
  inline-spacing pass for lines that contain none of the syntax characters its
  expressions can match. On the Substack body fixture, postprocessing fell
  from about 43.0 ms to 29.6 ms in release mode.
- `15460e2 Batch selector cleanup scans`: match exact cleanup selectors as a
  union, collect partial-cleanup candidates in one traversal, and cache the
  lowercased partial-selector patterns. On `wikipedia_html`, this reduced the
  median from about 20.2 ms to 17.5 ms and cumulative allocation from about
  7.88 MB / 138k allocations to 6.74 MB / 34k allocations.
- `84a2099 Reduce Markdown postprocess allocations`: preserve the existing
  input string when full-string regex passes find no matches. This reduced
  `substack_raw_markdown` cumulative allocation from 5.02 MB to 4.59 MB and
  peak live allocation from 1.65 MB to 1.22 MB, while the full benchmark run
  showed no meaningful timing regression.
- `Stateful Markdown postprocessing` (working tree): replace the chained
  full-string Markdown cleanup regexes with one line-oriented stateful pass.
  The pass carries fenced-code, blank-line, table, list, quote, and
  Hacker-News listing state while applying inline normalization directly to
  the output buffer. On the same 100-sample release benchmark,
  `substack_body_markdown` moved from 41.6 ms / 1.17 MB to 13.1 ms / 925 KB;
  fixture output remains byte-for-byte equivalent.
- `Selected-subtree Markdown DOM reuse`: clone only the selected content
  subtree, apply image/table/iframe normalization directly to that clone, and
  avoid reparsing the serialized fragment during Markdown preprocessing. The
  representative Markdown medians stayed within noise, while
  `substack_body_markdown` peak live allocation fell from about 925 KB to
  727 KB (about 21%) with unchanged fixture output.
- `AnalysisSnapshot` plus `MutationOverlay` milestone: add a profile-gated
  immutable subtree analysis IR in `crates/defuddle/src/analysis.rs` with
  stable `NodeId` mapping, parent/depth topology, tag membership, text/comma
  totals, descendant counts, link-text totals, structural flags, and
  checkpointed remove/tag/attribute mutations. The snapshot/overlay are
  currently validated against the mutable DOM oracle rather than replacing
  extraction behavior. On the 612-node Substack body fixture, snapshot
  construction measured about 1.0 ms per build across ten test iterations;
  overlay construction was below the displayed timer precision.
- `Route scoring through snapshot overlay`: `pipeline::score_and_remove()` now
  enumerates generic block candidates from the immutable snapshot and records
  removals in a mutation overlay before applying them at the end of each
  scoring round. The mutable DOM remains the scoring oracle, preserving the
  existing decision order and fixture output. Against the pre-routing
  100-sample reference, Substack HTML moved from 2.42 ms to 2.39 ms,
  Wikipedia HTML from 17.3 ms to 17.2 ms, and GitHub Markdown from 4.13 ms to
  4.05 ms; the remaining workloads stayed within normal variance.
- `Simple redirect retry guard` (working tree): detect static documents with
  one refresh meta tag and one body anchor, preserving the first-pass output
  while skipping retries that cannot add article content. On the ZIM corpus,
  this reduced retries from 2,425 to 1 and cumulative allocation from 12.78 GB
  to 9.24 GB in one-sample validation; the 218-fixture parity gate remained
  clean.
- `Direct partial-selector attribute scan` (working tree): collect elements
  carrying the partial-cleanup attributes directly instead of constructing and
  matching eight presence selectors. The ZIM phase report reduced selector
  removal from 9.10 s to 8.75 s, with unchanged fixture output.
- `Allocation-free scoring and standardization paths` (working tree): remove
  the unused production scoring snapshot/overlay allocation, store analysis
  tags as compact kinds, and avoid temporary lowercase strings in partial
  selectors, exact-selector attribute matching, and standardization. One-sample
  ZIM validation moved cumulative allocation from 9.18 GiB / 44.3M allocations
  to 9.00 GiB / 42.9M allocations; the full fixture parity gate remained
  clean.

The before/after end-to-end comparison for this architecture milestone is
intentionally flat: the snapshot and overlay are profile-gated while the
mutable DOM remains the production oracle. The current 100-sample medians are
`substack_body_markdown` 13.1 ms, `substack_raw_markdown` 16.0 ms,
`wikipedia_markdown` 21.1 ms, `substack_body_html` 2.45 ms, and
`wikipedia_html` 17.7 ms; the corresponding pre-milestone medians were within
normal benchmark noise. Profile-only construction measured about 0.97 ms for
the 612-node Substack subtree, with an empty overlay below displayed timer
precision.

## GOAL sequence

The lean baseline was freshly captured from `3515e05` with the required
100-sample release benchmark in `/tmp/defuddle-lean-baseline.txt`. The retained
sequence is measured in `/tmp/defuddle-final-sequence.txt`.

- Markdown presence guards in `markdown.rs`: skip WBR, footnote, task-list, and
  inline-formatting regex passes when their marker is absent. Fixture output is
  unchanged; `substack_body_markdown` moved from 13.14 ms to about 6.7 ms in
  the focused validation run, and `substack_raw_markdown` from 15.96 ms to
  about 9.4 ms before later generic optimizations.
- Protected selector pruning in `pipeline::remove_by_selector_protected()`:
  preserve the selected content subtree and its ancestors during document-wide
  cleanup. This eliminated the Wikipedia low-word-count retry (`0 -> 1840`
  words), saving about 10 ms on that workload while preserving all fixtures.
- Scoped compiled selector buckets in
  `dom::Document::query_selector_union_optimized_into()`: bucket static generic
  selector unions by final tag while leaving other DOM selector callers on the
  existing matcher. This reduced Wikipedia HTML from about 17.2 ms to 13.0 ms
  and cumulative allocation from 6.754 MB to 4.643 MB.

| Workload | Baseline median | Final median | Wall delta | Cumulative allocation |
| --- | ---: | ---: | ---: | ---: |
| `github_issue_html` | 3.469 ms | 3.585 ms | +3.3% | 1.052 MB → 1.052 MB |
| `github_issue_markdown` | 3.998 ms | 3.816 ms | -4.6% | 1.176 MB → 1.161 MB |
| `hackernews_comments_html` | 281.8 µs | 280.1 µs | -0.6% | 262.2 KB → 262.2 KB |
| `hackernews_comments_markdown` | 689.7 µs | 450.8 µs | -34.6% | 355.2 KB → 343.5 KB |
| `substack_body_html` | 2.385 ms | 2.372 ms | -0.5% | 808.7 KB → 808.7 KB |
| `substack_body_markdown` | 13.14 ms | 5.88 ms | -55.3% | 2.735 MB → 2.376 MB |
| `substack_raw_html` | 4.554 ms | 4.609 ms | +1.2% | 1.507 MB → 1.507 MB |
| `substack_raw_markdown` | 15.96 ms | 8.389 ms | -47.4% | 3.517 MB → 3.141 MB |
| `wikipedia_footnotes_html` | 3.166 ms | 2.942 ms | -7.1% | 3.915 MB → 3.932 MB |
| `wikipedia_footnotes_markdown` | 3.784 ms | 3.164 ms | -16.4% | 4.052 MB → 4.05 MB |
| `wikipedia_html` | 17.21 ms | 12.94 ms | -24.8% | 6.754 MB → 4.643 MB |
| `wikipedia_markdown` | 20.45 ms | 14.22 ms | -30.5% | 7.43 MB → 5.202 MB |

The remaining primary gap is `github_issue_markdown`, where the benchmark is
already within the no-regression guard but below the 20% target. GitHub's
profile is dominated by HTML parsing and site extraction rather than Markdown
preprocessing, so further Markdown-only changes are unlikely to reach that
 target without a broader DOM/parser optimization.

## Experiments not retained

- Storing parser text and comments in `html5ever::StrTendril` instead of
  `String` reduced allocation counts but did not improve cumulative allocation
  bytes or broad wall time. Peak live allocation rose on several representative
  workloads, so the storage change was discarded.
- Reserving a fixed `Vec<Node>` capacity during HTML parsing preserved fixture
  output but left representative medians mixed and did not produce a broad
  allocation win. It was discarded rather than imposing a fixed memory cost on
  small documents.
- The broad `2edd108 Complete generic snapshot overlay migration` was reverted
  by `3515e05 Revert broad snapshot overlay migration`. It preserved fixtures
  but added complexity without a broad wall-time win and increased large-page
  cumulative allocation; the lean baseline keeps the earlier scoring-overlay
  milestone only.
- Reusing one pre-collected `*` element list across all content-pattern passes
  preserved fixtures but moved the focused `wikipedia_html` run from 17.21 ms
  to 17.39 ms. The change was within noise and was discarded rather than
  retaining another phase-specific traversal cache.
- Applying the selector bucket globally through
  `Document::query_selector_union_into()` improved Wikipedia but pushed the
  focused `github_issue_html` run above the 5% guard. The retained
  `query_selector_union_optimized_into()` scope is limited to generic selector
  removal; site-extractor and standardization callers keep the existing
  matcher.
- Reusing the site extractor's mutable output DOM through
  `markdown::content_to_markdown_with_diagnostics()` instead of serializing to
  HTML and using `markdown::html_to_markdown_with_diagnostics()` preserved
  fixture output but regressed `substack_body_markdown` from 13.14 ms to
  14.82 ms and `substack_raw_markdown` from 15.96 ms to 17.33 ms. The selected
  document clone and preprocessing path is more expensive for these extractor
  outputs, so the change was discarded.
- A broad attempt to avoid all Markdown preprocessing was rejected because
  image, table, and iframe normalization changes output. The retained guard is
  limited to fragments where those transformations cannot apply.
- Preserving the input `String` when an inline-spacing regex had no match was
  measured at roughly 43.0 ms versus 42.6 ms for release postprocessing. That
  result was within noise, so the helper change was discarded.
- Fusing `text_content().to_lowercase()` into a single DOM traversal changed
  Wikipedia HTML from 34,044 allocations / 6.743 MB to 34,042 allocations /
  6.732 MB, while timing was slightly worse. The allocation difference was
  noise-level, so the extra lowercase traversal API was discarded.
- Combining lowercase conversion and space removal for partial attributes and
  hidden styles produced no measurable improvement on the representative
  fixtures. The normalization helpers were discarded rather than adding
  another abstraction for negligible savings.
- A batched selector plan for `scoring::find_main_content` was tested in two
  forms: one shared traversal and one tag/id/class candidate-bucket plan. The
  200-sample HTML medians stayed within noise (for example, Wikipedia was
  17.33 ms before and 17.65 ms with the bucket plan), while Markdown runs were
  roughly 3–5% slower from per-node candidate bookkeeping. Both versions were
  discarded; a broader selector index would need a better workload-specific
  plan before adding mutation invalidation complexity.
- A postorder `BlockFeatures` cache was tested for `pipeline::score_and_remove`.
  It removed repeated descendant count and heading scans but added a full
  subtree feature vector and traversal. Representative HTML medians remained
  flat to slightly worse (Substack body 2.47 ms to 2.49 ms; Wikipedia
  17.33 ms to 17.49 ms), so the cache was discarded rather than retaining a
  neutral abstraction.
- A second `BlockFeatures` implementation collected each scoring candidate's
  text during the same DFS, removing the follow-up `text_content()` scans. It
  preserved fixture output but increased Wikipedia cumulative allocation from
  about 6.75 MB to 7.0 MB and left Substack timing flat, so retaining the text
  buffers was not worth the complexity.
- Adding `outer_html_into()` / `inner_html_into()` and using them at the main
  serialization and footnote sibling-assembly sites did not change peak live
  allocation. `wikipedia_footnotes_markdown` moved from about 3.77 ms to
  3.81 ms at 200 samples with the same 787 KB peak, so the API-only buffer
  reuse experiment was discarded; meaningful reuse needs a larger ownership
  or lifetime change.
- Compacting repeated DOM tag names into enum-backed storage reduced peak live
  memory slightly but regressed Wikipedia HTML by several percent and did not
  improve the representative Markdown suite. It was reverted because the
  storage complexity did not buy a meaningful end-to-end gain.
- Interning common DOM attribute names produced roughly a 1% memory reduction
  on Substack HTML but was neutral to slightly slower on wall time and did not
  improve cumulative allocations enough to justify the broader attribute API
  conversion. It was reverted.
- Replacing per-node child `Vec<NodeId>` allocations with `SmallVec` inline
  storage reduced HTML allocations by roughly 2–4% and improved Substack HTML
  by about 2.5%, but made Substack and raw Markdown roughly 3–5% slower. The
  experiment was reverted under the no-regression rule.
- A document-wide simple-tag index was tested to bypass repeated scans for
  selectors such as `p`, `a`, and `article`. The first implementation failed
  fixture parity across 49 active fixtures because tag renames and DOM
  mutations invalidated the index. It was reverted immediately; any future
  index must be mutation-aware and preserve document-order semantics. A second
  version updated the index on tag renames, but still failed 19 fixtures due to
  ordering/DOM mutation interactions and was also reverted.
- A mutation-safe metadata-only tag index was then rebuilt after pruning and
  invalidated before extraction mutations. It passed the fixture gate but was
  noise-level on Wikipedia (about 17.24 ms versus 17.33 ms) and slightly
  slower on raw Substack HTML, so the rebuild/invalidation machinery was also
  discarded.
- A pristine `Document` clone was retained before the first extraction so
  retries could reuse it instead of reparsing the source HTML. Fixture parity
  passed, but ordinary Substack Markdown rose from about 13.3 ms to 14.8 ms
  and Wikipedia peak memory increased, so clone-based retry reuse was reverted.
- PGO is deliberately deferred. The end-to-end fixture suite is shaped so the
  same `bench` binary can later drive PGO, but no profile-generation or
  profile-use flags should be inferred from the current workflow.
