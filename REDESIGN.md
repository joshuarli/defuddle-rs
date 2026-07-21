# Pipeline Redesign: DOM-Backed Site Extraction

This plan moves `crates/defuddle` toward a disciplined, pluggable pipeline while keeping the first implementation deliberately narrow.

The proof of concept is the Hacker News extractor. It will be migrated end to end, measured against the current implementation, and used to validate the architecture before the other extractors move.

## Current baseline after cleanup

The cleanup has already established several ownership boundaries that the redesign should preserve:

- `extractors/` owns site-specific DOM interpretation and currently returns an `ExtractorResult` containing serialized HTML.
- `standardize/` owns generic DOM normalization, with focused submodules for elements, headings, images, tables, URLs, whitespace, and callouts.
- `footnotes/` owns footnote discovery and canonical DOM construction; Markdown rendering of the canonical form remains at the output boundary.
- `embeds/` owns static platform-embed normalization, currently YouTube and Twitter/X iframe handling.
- Renderer-specific code-block heuristics, BBCode, and math/LaTeX handling are intentionally out of scope and should not be reintroduced as generic pipeline stages.

The remaining architectural problem is therefore not “split every special case into a module.” It is the string boundary between site extraction and the generic DOM pipeline, plus the lack of explicit context, diagnostics, and end-to-end performance measurement.

## Target MVP

The MVP has one defining constraint:

> HTML is parsed once, remains DOM-backed through extraction and cleanup, and is serialized only at the output boundary.

The internal pipeline should have these boundaries:

```text
HTML
  → Document + PipelineContext
  → site adapter selection
  → site preparation / extraction
  → generic cleanup stages (including standardize, footnotes, callouts, embeds, sanitize)
  → DOM-backed content result
  → HTML / Markdown serialization
```

The first version does not need a new crate, a dynamic plugin system, or a complete rewrite of the generic pipeline.

## Stage 1: Establish the measurement baseline

- [x] Add a repeatable benchmark harness for end-to-end extraction.
  - Benchmark at least the Hacker News fixture, one ordinary article fixture, and one extractor-free document.
  - Measure both HTML output and Markdown output.
  - Report throughput and input size; keep fixture inputs vendored and deterministic.
- [x] Add a correctness baseline alongside the benchmark cases.
  - Compare metadata, HTML, and Markdown against the current golden outputs.
  - Include Hacker News listings, posts, and nested comments if representative fixtures exist.
- [x] Add profiling instructions and a profile-friendly benchmark command.
  - Document release-mode profiling with `cargo flamegraph` or an equivalent platform profiler.
  - Identify the top-level phases to inspect: parsing, metadata, site extraction, standardization, Markdown conversion.
- [x] Record the baseline in this document or a checked-in benchmark summary.

Acceptance gate: `cargo test -p defuddle` passes, benchmarks are repeatable, and a profile can distinguish the major pipeline phases.

The benchmark command is:

```bash
cargo bench -p defuddle --bench baseline
```

It runs 10 warmup iterations and 100 measured iterations for five vendored fixtures: Hacker News listing, post, and nested comments; an ordinary Wikipedia article; and an extractor-free footnote document. Each case compares the metadata preamble and Markdown against its checked-in golden. Cases with an HTML golden compare that output too. The benchmark reports input/output sizes, median latency, and input-throughput for HTML-only and Markdown-enabled extraction.

For a release-mode profile, install `cargo-flamegraph` and run:

```bash
cargo flamegraph -p defuddle --bench baseline -- --profile baseline.svg
```

The custom benchmark is intentionally profile-friendly: it performs the correctness checks once, then repeats only extraction in the timed loop. Inspect these top-level phases in the resulting profile: parsing (`Document::parse`), metadata/schema (`extract_metadata` and `extract_schema_org_data`), site extraction (`extractors`), generic standardization (`standardize`), and Markdown conversion (`markdown`). On macOS, an equivalent Instruments Time Profiler recording can launch `target/release/deps/baseline-*` after `cargo bench -p defuddle --bench baseline --no-run`.

See [`benchmarks/BASELINE.md`](benchmarks/BASELINE.md) for the recorded run.

## Stage 2: Introduce the internal pipeline boundary

- [x] Add a `PipelineContext` containing the working `Document`, URL information, metadata, options, and diagnostics.
- [x] Add a DOM-backed content result, initially using the existing `Document` and `NodeId` types rather than introducing a second DOM abstraction.
- [x] Change the standard pipeline to consume and return the context/content boundary explicitly.
- [x] Keep existing focused boundaries (`standardize/callouts/`, `footnotes/`, and `embeds/`) intact while changing ownership around them; do not fold them back into a monolithic standardizer.
- [x] Keep public `DefuddleResult` unchanged; HTML and Markdown serialization remain output concerns.
- [x] Add phase timing hooks to the context or a small diagnostics structure so benchmarks and profiling can attribute work to stages without logging by default.

Acceptance gate: behavior is unchanged, the full fixture suite passes, and the benchmark reports the same baseline phases without material regression.

## Stage 3: Migrate Hacker News as the proof of concept

- [x] Define a `SiteAdapter` boundary with `matches`, `prepare`, and `extract` responsibilities.
- [x] Adapt the registry to select a Hacker News adapter without exposing Hacker News details to the generic pipeline.
- [x] Make Hacker News extraction return a DOM-backed content result instead of an HTML string.
  - [x] Move or clone selected post/comment nodes into the content root.
  - [x] Preserve the existing metadata and nested comment behavior.
  - [x] Keep generated headings or separators as DOM nodes, not serialized HTML fragments.
- [x] Remove Hacker News-specific reparsing, HTML concatenation, and generic-pipeline conditionals.
- [x] Define how shared stages such as footnotes, callouts, embeds, sanitization, and Markdown serialization consume the adapter-owned content result.
- [x] Add adapter contract tests for listing pages, post pages, nested comments, empty comments, and malformed/missing optional fields.

Acceptance gate: Hacker News HTML, Markdown, metadata, and all existing fixture outputs are byte-for-byte compatible unless a difference is explicitly justified. Benchmark and profile the migrated path against the Stage 1 baseline.

## Stage 4: Validate the architectural MVP

- [x] Compare before/after end-to-end performance for Hacker News and the non-Hacker-News controls.
  - Throughput and median latency must be reported.
  - Investigate allocation or memory regressions where the profiler makes them visible.
- [x] Capture a before/after profile for the Hacker News path.
  - Confirm that extraction no longer spends time reparsing its own generated HTML.
  - Confirm that any new DOM movement cost is understood and bounded.
- [x] Add an invariant test or debug assertion that site extraction does not require serialized HTML to continue through the pipeline.
- [x] Document the adapter contract, context ownership, output boundary, and benchmark commands for future extractor migrations.
- [x] Decide whether the boundary is good enough to migrate the next extractor. If not, fix the boundary before adding more adapters.

Acceptance gate: all core tests pass, Hacker News parity passes, benchmark results are recorded, and profiling demonstrates the intended architectural change rather than merely moving work around.

Stage 4 validation is recorded in [`benchmarks/STAGE4.md`](benchmarks/STAGE4.md). The boundary is good enough to migrate one additional extractor in Stage 5. The next migration should retain the existing legacy `Extractor` path as a control until the adapter contract has been exercised by a second content shape.

## Stage 5: Generalize only after the MVP is proven

- [x] Migrate Substack as the second adapter, covering article markup, Notes markup, ordinary body fallback, and embedded preload JSON content.
- [x] Move Substack-specific preparation into adapter-owned DOM transforms and keep generic cleanup site-agnostic.
- [ ] Convert `ExtractorResult` and remaining string-based extractor boundaries incrementally.
- [ ] Split modules or crates only where the new ownership boundaries make the split obvious.

Do not start this stage until Stage 4 has produced a validated Hacker News architecture.

## Design constraints

- Preserve the public API during the MVP.
- Do not rewrite the DOM adapter as part of this effort.
- Do not introduce async extraction, network access, or a general-purpose plugin loading system.
- Do not optimize based on isolated microbenchmarks; use end-to-end extraction and profiles.
- Do not encode fixture-specific behavior in `PipelineContext` or `SiteAdapter`.
- Keep serialization at the edge. Generated HTML strings are acceptable only at the final result boundary or for genuinely external output formats.

## Deferred questions

- Whether `SiteAdapter` should eventually become a public extension API.
- Whether generic cleanup stages should become independently registered plugins.
- Whether a separate `defuddle-extractors` crate is justified after multiple adapters share the new boundary.
- Whether the DOM adapter needs a first-class `ContentFragment` type instead of `(Document, NodeId)`.
