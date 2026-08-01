# Performance

Defuddle performance work is measured through the public `parse_html()` path
on representative HTML fixtures. The benchmark binary uses Divan and the
system allocator wrapped by `AllocProfiler`; it is intentionally useful as a
future PGO workload, but PGO is not currently part of the build or release
workflow.

## Core workflow

Run the complete benchmark set with:

```bash
make bench
```

The entry point is the `bench` target in `Makefile`, which runs
`crates/defuddle/benches/bench.rs` in the optimized Cargo profile. The suite
covers HTML and Markdown output for local Substack captures and vendored
upstream Wikipedia, GitHub, Hacker News, and footnote fixtures.

For repeatable comparisons, keep the source revision, Rust toolchain, machine,
allocator, and benchmark options constant. Capture both revisions with the
same sample count, for example:

```bash
cargo bench -p defuddle --bench bench -- --sample-count 100 \
  > /tmp/defuddle-bench.txt
```

Divan accepts a positional name filter, so a focused run looks like:

```bash
cargo bench -p defuddle --bench bench -- substack_raw_markdown --sample-count 100
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

Divan reports two useful allocation views for each timed operation:

- `max alloc` is the high-water mark of live allocations during one operation.
  It is a peak live-allocation measurement, not process RSS.
- `alloc` is cumulative allocation count and bytes over the operation. It
  includes allocations that have already been freed and is useful for finding
  churn.

The separate `dealloc` and `grow` rows help explain allocator behavior. Use
`/usr/bin/time -l` or a platform profiler when process RSS or the total process
footprint is needed; those measurements include the runtime and benchmark
harness.

## Current reference results

These are representative medians from the optimized tree, with 100 Divan
samples on the development machine. They are a reference point, not a
cross-machine performance guarantee.

| Workload | Median | Peak live allocation |
| --- | ---: | ---: |
| `substack_body_html` | 2.42 ms | 520 KB |
| `substack_body_markdown` | 13.1 ms | 727 KB |
| `substack_raw_html` | 4.73 ms | 1.01 MB |
| `substack_raw_markdown` | 16.1 ms | 1.01 MB |
| `wikipedia_html` | 17.3 ms | 1.01 MB |
| `wikipedia_markdown` | 20.6 ms | 1.18 MB |
| `github_issue_markdown` | 4.13 ms | 532 KB |
| `hackernews_comments_markdown` | 703 µs | 124 KB |

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

## Experiments not retained

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
