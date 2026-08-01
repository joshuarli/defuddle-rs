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
| `substack_body_html` | 2.45 ms | 520 KB |
| `substack_body_markdown` | 54.4 ms | 1.59 MB |
| `substack_raw_html` | 4.70 ms | 1.01 MB |
| `substack_raw_markdown` | 57.3 ms | 1.65 MB |
| `wikipedia_html` | 20.3 ms | 1.01 MB |
| `wikipedia_markdown` | 37.5 ms | 1.40 MB |
| `github_issue_markdown` | 6.69 ms | 532 KB |
| `hackernews_comments_markdown` | 2.14 ms | 124 KB |

The dominant remaining wall-time path is Markdown conversion, especially
`markdown::html_to_markdown()` and its `postprocess_markdown()` cleanup around
the `htmd` conversion. Generic extraction is dominated by repeated DOM
selection and cleanup passes; direct subtree traversal now removes the most
expensive whole-document containment scan. Peak memory is highest when a large
source DOM and a generated extractor DOM overlap, or when Markdown conversion
adds its parser and intermediate strings.

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

## Experiments not retained

- A broad attempt to avoid all Markdown preprocessing was rejected because
  image, table, and iframe normalization changes output. The retained guard is
  limited to fragments where those transformations cannot apply.
- Rewriting the entire Markdown postprocessing regex chain was not kept as a
  speculative refactor. Profiling identifies it as a hotspot, but a rewrite
  needs output-equivalence coverage and a clear improvement beyond noise before
  adding that complexity.
- PGO is deliberately deferred. The end-to-end fixture suite is shaped so the
  same `bench` binary can later drive PGO, but no profile-generation or
  profile-use flags should be inferred from the current workflow.

