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
| `substack_body_html` | 2.44 ms | 520 KB |
| `substack_body_markdown` | 41.6 ms | 1.17 MB |
| `substack_raw_html` | 4.67 ms | 1.01 MB |
| `substack_raw_markdown` | 44.5 ms | 1.22 MB |
| `wikipedia_html` | 17.5 ms | 1.01 MB |
| `wikipedia_markdown` | 34.7 ms | 1.27 MB |
| `github_issue_markdown` | 6.48 ms | 532 KB |
| `hackernews_comments_markdown` | 1.69 ms | 124 KB |

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

## Experiments not retained

- A broad attempt to avoid all Markdown preprocessing was rejected because
  image, table, and iframe normalization changes output. The retained guard is
  limited to fragments where those transformations cannot apply.
- Rewriting the entire Markdown postprocessing regex chain was not kept as a
  speculative refactor. Profiling identifies it as a hotspot, but a rewrite
  needs output-equivalence coverage and a clear improvement beyond noise before
  adding that complexity.
- Preserving the input `String` when an inline-spacing regex had no match was
  measured at roughly 43.0 ms versus 42.6 ms for release postprocessing. That
  result was within noise, so the helper change was discarded.
- PGO is deliberately deferred. The end-to-end fixture suite is shaped so the
  same `bench` binary can later drive PGO, but no profile-generation or
  profile-use flags should be inferred from the current workflow.
