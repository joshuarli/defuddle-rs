# Defuddle Rust Port Plan

## Goal

Reimplement Defuddle as a Rust library using the TypeScript project in `../defuddle` as the behavioral reference. The port should be test-driven: move as much of the existing test suite as practical into Rust first, then implement in strict tranches until the Rust suite reaches parity.

The TypeScript implementation has one real environment advantage: the browser build can operate on a live `Document`, including real `getComputedStyle`, `styleSheets`/media rules, readable shadow roots, and browser DOM behavior. The Rust v1 should target static HTML parity with the Node/linkedom path first. Browser-only behavior can be added later through explicit caller-provided hooks rather than by embedding a browser engine.

## Non-Negotiable Rules

- Tests are the contract. Do not hand-tune behavior without either porting the corresponding upstream test or adding a focused Rust test that captures the intended behavior.
- Golden fixture expectations come from `../defuddle/tests/expected`. Do not regenerate Rust goldens independently unless the TypeScript reference output has changed and the diff is intentionally accepted.
- Each tranche must end with a passing gate before starting the next tranche.
- Any unsupported upstream test must be listed in an explicit exception manifest with a reason and the missing capability.
- Keep the library API small until the extraction behavior is proven. Avoid premature abstractions and avoid optional dependencies unless they are required by tests.

## Explicit Non-Scope

The Rust v1 port is a static-HTML article extraction library, not a full rewrite of every package surface in `../defuddle`.

Out of scope for the core crate:
- Website and hosted product code under `../defuddle/website`.
- NPM package concerns: webpack bundles, browser/full/node entrypoint packaging, TypeScript declaration builds, bundle-size scripts, and publish scripts.
- Node compatibility shims such as `linkedom-compat`, JSDOM-specific behavior, and JavaScript package ergonomics.
- Built-in page fetching and proxy support from `src/fetch.ts`. The core crate accepts HTML input only. It must not depend on HTTP clients, proxy libraries, TLS stacks, or URL-fetching abstractions.
- Live browser integration: real layout/computed CSS, `styleSheets` media-rule evaluation, runtime JavaScript execution, readable closed/isolated shadow DOM, DOM clicking, `MutationObserver`, and waiting for sites to render panels.
- Exact `parseTime` parity. Timing fields can exist, but tests should not assert specific timings.
- Heavy optional math conversion that depends on `mathml-to-latex` or `temml`. The core should preserve and normalize math where practical; advanced conversion is optional future work.
- URL-as-input behavior. Any URL accepted by the API or CLI is metadata/base-URL context only, never an instruction to fetch.

Out of scope for the port unless explicitly re-scoped later:
- Bilibili extraction and `tests/bilibili-transcript.test.ts`.
- The Bilibili API/subtitle model, host allowlist, language-track selection, and CJK subtitle grouping specific to that extractor.

Out of scope for the core crate, but possible future work in the separate extractor crate:
- Platform/site extractors other than the transcript/YouTube work called out below, including Twitter/X, X oEmbed, Reddit, Hacker News comments, ChatGPT, Claude, Gemini, Grok, GitHub, LinkedIn, Threads, Bluesky, Mastodon, Discourse, Medium, Substack, New York Times, Wikipedia, LeetCode, LWN, C2 Wiki, and BBCode-data extraction.
- Their focused tests should be represented in the exception manifest until a given extractor is explicitly accepted into scope.

## Workspace And Release Targets

Use a workspace so scope boundaries are visible in Cargo metadata:
- `defuddle`: the HTML-only library. This crate has no network dependencies and is the primary v1 target.
- `defuddle-cli`: a thin `lexopt` binary that demonstrates the core library. It may depend on the core crate and filesystem/stdin/stdout only.
- `defuddle-extractors`: a separate optional crate for transcript/media extractors. It must not be required by the core crate.
- Shared test helpers may live in a private workspace crate only if they avoid becoming a second public API.

Release targets:
- Core library v1 requires Tranches 0-5 and 8.
- CLI v1 requires core library v1 plus both commands from Tranche 9: HTML-only `parse` and static/mocked `extract youtube`.
- Extractor v1 requires transcript utilities plus YouTube static/mocked behavior and is included in the v1 CLI release.
- Combined release readiness is verified in Tranche 10.
- Site extractor compatibility outside YouTube is optional and must be explicitly re-scoped before implementation.

Dependency gates:
- The core crate must not include HTTP client, TLS, proxy, async runtime, browser automation, or JavaScript engine dependencies.
- The CLI crate must not add network dependencies.
- The extractor crate may define host-fetch traits/request types, but must not add HTTP client/TLS/proxy dependencies unless a later plan revision explicitly changes this.
- Any new dependency requires a short note explaining why the standard library or an existing dependency is insufficient.

## Test And Fixture Policy

Fixture strategy:
- Vendor a pinned snapshot of `../defuddle/tests` into this repository before enabling the fixture harness.
- Record the source upstream commit or local source snapshot details used for the vendored fixtures.
- During later upstream syncs, update the vendored snapshot intentionally and review resulting fixture diffs.
- Never silently rewrite expected outputs. If the TypeScript reference output changes, record why the Rust expectation changed.

Exception manifest:
- Maintain a machine-readable manifest, for example `tests/upstream-exceptions.toml`.
- Each exception must include: upstream test or fixture name, category, reason, required capability, target tranche or `out_of_scope`, and date added.
- Allowed categories: `browser_only`, `network_only`, `site_extractor_out_of_scope`, `bilibili_excluded`, `cli_fetch_excluded`, `dependency_blocked`, `intentional_api_difference`.
- A tranche acceptance gate cannot pass if new uncategorized failures are present.

Golden comparison policy:
- Match TypeScript fixture tests byte-for-byte after the same trimming policy wherever practical.
- If parser serialization differences make byte-for-byte HTML unstable, add a documented HTML normalizer and use it consistently; do this only after real fixture failures prove it is needed.
- Do not normalize Markdown unless the TypeScript test does.
- Metadata JSON preambles should use the same field set and ordering as the TypeScript fixture test.
- Timing fields are excluded from parity assertions.

## Tranche 0: Project Skeleton and Dependency Spike

Deliverable: a compiling Rust crate with no Defuddle behavior beyond type definitions and placeholder parsing.

Implementation scope:
- Create a library crate in `defuddle-rs`.
- Define initial public types: `DefuddleOptions`, `DefuddleResult`, metadata fields, debug removals, and error type.
- Add a single `parse_html(html: &str, options: DefuddleOptions) -> Result<DefuddleResult, DefuddleError>` entry point.
- Evaluate the minimum viable HTML stack with a small spike:
  - parsing full documents,
  - CSS selector queries,
  - mutation/removal/replacement,
  - cloning or reparsing,
  - serializing inner/outer HTML,
  - preserving enough source semantics for fixture parity.
- Use `html5ever` as the parser/serializer foundation.
- Build or adopt a small mutable DOM adapter over the parsed tree, with only the operations Defuddle needs: selector queries, ancestor/descendant traversal, `closest`, `contains`, attribute access/mutation, text content, remove/unwrap/replace/append, clone/reparse, and inner/outer HTML serialization.
- Do not use `lol_html` as the primary implementation. It is a streaming rewriter, while Defuddle needs whole-document scoring, retries, external footnote adoption, subtree comparison, adjacent-node/image dedupe, and selected-element serialization.
- Do not use `html5gum` as the primary implementation. It is useful as a tokenizer, but the port needs tree construction, mutation, selector matching, and serialization.
- Do not expose or rely directly on `markup5ever_rcdom` as the production DOM model. If used during the spike, wrap or replace it with a Defuddle-owned adapter before core implementation proceeds.
- Use `scraper` only if it can satisfy mutation and serialization requirements cleanly; otherwise keep selector support separate from the DOM adapter.
- Evaluate a small HTML-to-Markdown crate for Tranche 5 and document whether it can support Defuddle's custom table, footnote, math, callout, media, and escaping rules.

Acceptance gate:
- `cargo test` passes.
- A written dependency note exists in the repo documenting the `html5ever`-based stack and why `lol_html`, `html5gum`, raw `RcDom`, and any rejected selector/DOM candidates were not chosen.
- The dependency note also records the chosen Markdown crate or states that no suitable crate was found and the plan must be revised before Tranche 5.
- DOM adapter spike tests demonstrate parse, select, traverse ancestors, remove, unwrap, replace, append, clone/reparse, mutate attributes, text extraction, and inner/outer HTML serialization.
- `cargo tree` or equivalent confirms the core crate has no network, TLS, proxy, async runtime, browser automation, or JavaScript engine dependencies.
- No fixture tests are enabled yet.

## Tranche 1: Port the Test Harness Before Behavior

Deliverable: Rust can discover and run the upstream fixture suite against a placeholder implementation, with failures reported in a useful way.

Implementation scope:
- Vendor the upstream fixture inputs and expected outputs from `../defuddle/tests`:
  - all `tests/fixtures/*.html`,
  - all `tests/expected/*.md`,
  - all `tests/expected/*.html`.
- Record the source snapshot used for the vendored test data.
- Implement a Rust golden fixture runner equivalent to `../defuddle/tests/fixtures.test.ts`.
- Parse fixture URL frontmatter comments exactly as the TypeScript test does.
- Compare output in the same shape as TypeScript:
  - JSON preamble with `title`, `author`, `site`, and `published`,
  - followed by Markdown content,
  - plus HTML comparison where an expected `.html` file exists.
- Add an exception manifest for tests that are initially excluded. Each entry must include fixture name, reason, and tranche expected to enable it.

Acceptance gate:
- `cargo test --test fixtures` runs all discoverable fixture cases.
- The runner reports deterministic failures without panics.
- The exception manifest exists, validates against the schema above, and contains no uncategorized failures.
- No extraction implementation is required to pass fixture assertions yet, but the harness itself must be complete.

## Tranche 2: Core Text, Metadata, and Serialization Parity

Deliverable: Rust can parse static HTML, extract basic metadata, count words, sanitize dangerous markup, resolve URLs, and serialize stable HTML.

Implementation scope:
- Port core utilities:
  - CJK-aware word counting,
  - text normalization,
  - HTML entity decoding,
  - dangerous URL detection,
  - relative URL and `srcset` resolution,
  - schema.org JSON-LD extraction,
  - meta tag collection.
- Port metadata extraction from `src/metadata.ts` for title, author, site, description, domain, favicon, image, language, and published date.
- Port sanitizer behavior for script/style/noscript/object/embed/applet/base removal, event handler stripping, `srcdoc` stripping, and dangerous URI stripping.
- Add focused unit tests before each utility module is implemented.

Acceptance gate:
- `cargo test` passes.
- All focused utility/metadata/sanitizer tests pass.
- A fixture subset selected for metadata-only confidence passes at the metadata preamble level.
- No Markdown/content fixture is marked passing unless its content is genuinely produced by Rust logic.

## Tranche 3: Generic Main-Content Extraction

Deliverable: Rust selects the same main content container for ordinary article pages and returns cleaned HTML, without Markdown conversion as the primary target.

Implementation scope:
- Port the synchronous generic pipeline from `src/defuddle.ts`:
  - option defaults,
  - retry flow for short content,
  - content selector override,
  - schema text fallback,
  - entry-point candidate selection,
  - table-based content fallback,
  - block scoring.
- Port deterministic removals:
  - exact and partial selector removals,
  - inline-style/class hidden removal,
  - small image removal,
  - metadata block removal,
  - content-pattern removal where needed by enabled fixtures.
- Do not implement live browser computed CSS in this tranche. Keep this explicitly different from the browser TypeScript path.

Acceptance gate:
- `cargo test` passes.
- At least 40 representative non-extractor fixtures pass HTML or normalized-content assertions.
- Debug output can report the selected content selector and removals for failed fixtures.
- Any fixture failure in this tranche must be attributable to a known unported standardization, Markdown, extractor, or browser-only feature.

## Tranche 4: HTML Standardization

Deliverable: Rust normalizes extracted HTML closely enough for downstream Markdown fixture parity.

Implementation scope:
- Port standardization modules in test-driven order:
  - headings and permalink cleanup,
  - code block normalization,
  - image/lazy image handling and deduplication,
  - callouts,
  - footnotes,
  - layout table handling,
  - math preservation and lightweight normalization.
- Preserve useful comments from the TypeScript source when porting non-obvious behavior.
- Add focused tests for each standardization module before broadening fixture expectations.

Acceptance gate:
- `cargo test` passes.
- The generic non-extractor fixture set reaches at least 60% full Markdown parity.
- All focused standardization tests pass.
- Remaining failures are categorized in the exception manifest by missing Markdown rules, site extractor behavior, async/network behavior, or browser-only behavior.

## Tranche 5: Markdown Conversion

Deliverable: Rust emits Markdown matching the upstream expected fixture files for generic pages.

Implementation scope:
- Use a small vetted HTML-to-Markdown crate as the base renderer if the spike shows it can support Defuddle's required custom rules.
- Keep the dependency note current with why the selected crate was chosen and how custom rules are layered on top.
- If no crate can support the required rules without invasive work, stop and revise the plan before writing a bespoke renderer.
- Port Markdown rules from `src/markdown.ts`:
  - headings,
  - links and images,
  - simple and complex tables,
  - code fences,
  - footnotes,
  - callouts,
  - math,
  - preserved media elements,
  - escaping rules for tag-like text.
- Keep Markdown generation deterministic and independent from browser behavior.

Acceptance gate:
- `cargo test` passes.
- At least 80% of all portable fixture `.md` expectations pass byte-for-byte after trimming, using the same comparison policy as TypeScript.
- Every failing portable fixture has a linked focused test or a specific implementation task.
- No site-specific extractor is required for this gate unless the fixture cannot pass through generic extraction.

## Tranche 6: Optional Deterministic Site Extractor Compatibility

Deliverable: if explicitly accepted after core parity, Rust matches TypeScript behavior for selected site-specific extractors that can run entirely from provided HTML. This tranche is not required for the core crate v1.

Implementation scope:
- Port extractor registry and extractor result model.
- Implement only selected DOM-only extractors. Candidate order, if this tranche is accepted:
  - Hacker News,
  - GitHub,
  - Wikipedia,
  - ChatGPT,
  - Claude,
  - Gemini,
  - Grok,
  - Medium,
  - Substack static paths,
  - Reddit old/new DOM paths,
  - Twitter/X DOM paths,
  - other fixture-backed extractors that do not require live network.
- Keep async/network extractor behavior disabled in fixture tests unless a deterministic mock is provided.
- Do not implement Bilibili in this tranche.

Acceptance gate:
- `cargo test` passes.
- All fixture expectations for selected in-scope extractors pass.
- Extractor type and variables match upstream where expected.
- Exception manifest continues to list every unselected extractor test as intentionally out of scope.

## Tranche 7: Optional Host-Fetch Extractor Compatibility

Deliverable: if explicitly accepted after deterministic extractor work, Rust can model selected fetch-backed extractor behavior through caller-provided mocks or host integration, without adding HTTP client dependencies to the crates.

Implementation scope:
- Add optional async API only after synchronous parity is strong:
  - `parse_html_async(...)` or equivalent,
  - caller-provided fetch abstraction with request/response types owned by the extractor crate,
  - timeout and size-limit policy represented in the host-facing API but not implemented through a bundled HTTP client.
- Port selected fetch-backed extractor behavior such as YouTube, X oEmbed, Reddit fallback, and similar paths only when tests provide mocked responses.
- Port transcript parsing tests and network-surrogate tests with mocked responses.
- Do not port Bilibili fetch/subtitle behavior.

Acceptance gate:
- `cargo test` passes with async tests enabled.
- All accepted upstream tests that can be made deterministic with mocked fetch pass.
- Network is never required for default tests, CLI tests, or crate tests.
- The Rust crates do not depend on HTTP client/TLS/proxy crates.
- Any remaining exceptions are browser-only or intentionally out of scope for v1.

## Tranche 8: Public API And Performance Stabilization

Deliverable: a usable Rust library API with stable behavior, documentation, and measured performance.

Implementation scope:
- Finalize public API names and serialization types.
- Add crate documentation and examples for HTML input, URL option, Markdown output, and debug output.
- Add benchmarks against a representative fixture set.
- Compare Rust throughput and allocation behavior against the TypeScript Node path.
- Add feature flags only where they map to real optional capabilities, such as extractor support or heavy math conversion.

Acceptance gate:
- `cargo test` passes.
- Portable upstream test parity is complete.
- Benchmarks show no obvious performance regression against TypeScript for static HTML parsing.
- Public API docs compile.
- Exception manifest is empty for core-library v1 scope, or contains only explicitly documented non-goals.

## Tranche 9: Minimal `lexopt` CLI

Deliverable: a small CLI binary that demonstrates the core library and, when built with the extractor crate, the separate extractor surface. The CLI is not allowed to fetch URLs.

Implementation scope:
- Use `lexopt` for argument parsing; do not add `clap`, `commander`-style frameworks, or network dependencies.
- Copy Defuddle's CLI ergonomics where they fit the HTML-only scope:
  - `defuddle parse [source]`,
  - `source` is a local HTML file or `-`/omitted stdin,
  - `-o, --output <file>`,
  - `-m, --markdown`,
  - `--md`,
  - `-j, --json`,
  - `-f, --frontmatter`,
  - `-p, --property <name>`,
  - `--debug`,
  - `-l, --lang <code>`.
- Replace Defuddle's URL-fetching behavior with explicit base-URL metadata:
  - `--url <url>` sets the document URL/base URL used for metadata and relative URL resolution.
  - Passing `https://...` as `source` is an error with a message explaining that the CLI only reads HTML files/stdin and that `--url` is metadata context.
  - Do not implement `--user-agent`; there is no HTTP fetch path.
- Preserve output behavior from Defuddle where possible:
  - raw HTML by default,
  - Markdown when `--markdown`/`--md` is set,
  - JSON with metadata/content when `--json` is set,
  - YAML frontmatter when `--frontmatter` is set,
  - selected field output when `--property` is set,
  - write to stdout unless `--output` is set.
- Add an extractor demonstration command behind an extractor feature or separate binary:
  - `defuddle extract youtube [source] --url <youtube-url>`,
  - `source` is static YouTube HTML from file/stdin,
  - output supports the same `--json`, `--property`, and `--output` conventions,
  - no network-backed transcript lookup is performed by the CLI.

Acceptance gate:
- `cargo test` passes.
- CLI tests cover stdin, `-`, local file input, TTY/no-input error, URL-as-source rejection, `--url` metadata context, Markdown output, JSON output, frontmatter output, property output, and output-file writing.
- Extractor CLI tests cover static YouTube HTML input and prove the command does not require network access.
- The CLI remains a thin adapter: parsing arguments, reading/writing files/stdin/stdout, calling library APIs, and formatting errors.

## Tranche 10: Release Readiness

Deliverable: releaseable core and CLI artifacts with scope boundaries verified.

Implementation scope:
- Finalize README examples for library and CLI usage.
- Confirm docs clearly state that the core library processes HTML strings and does not fetch URLs.
- Confirm feature flags and workspace members are documented.
- Confirm upstream test exceptions are intentional and current.
- Run formatting, linting, tests, fixture parity tests, docs, and benchmarks.

Acceptance gate:
- Core crate, CLI crate, and enabled extractor crate tests pass.
- Core and CLI dependency audits confirm no network stack dependencies.
- Documentation examples compile or are covered by integration tests.
- Exception manifest has no unexplained core-v1 failures.
- Release notes list included v1 surfaces: core HTML parsing, HTML-only CLI parsing, and static/mocked YouTube extraction.
- Release notes list intentionally excluded surfaces: URL fetching, proxy support, browser live-DOM behavior, Bilibili, and unselected site extractors.

## Definition of Portable Parity

A test is portable if it uses static HTML input plus deterministic mocked fetch responses and does not depend on:
- a live browser layout engine,
- true computed CSS from external stylesheets,
- runtime JavaScript execution,
- inaccessible shadow DOM from an extension isolated world,
- live network responses.

Portable core tests should pass in Rust. Site-specific extractor tests are portable only after that extractor is explicitly accepted into scope. Network-backed tests are not portable unless the network behavior is modeled entirely by caller-provided mocks and the Rust crates still do not include HTTP client dependencies. Non-portable or out-of-scope behavior should be documented, not silently approximated.

## Initial Test Inventory

The upstream repository currently has:
- 202 HTML fixtures in `../defuddle/tests/fixtures`.
- 202 Markdown expected files in `../defuddle/tests/expected`.
- 3 HTML expected files in `../defuddle/tests/expected`.
- Additional focused tests for CLI, fetch, Markdown, media removal, schema fallback, no-mutation behavior, SVG sanitization, transcripts, oEmbed surrogates, extractor XSS, and performance.
- `tests/bilibili-transcript.test.ts` is explicitly excluded from the Rust port.

Initial test disposition:
- Core crate inclusion targets: fixture harness, `debug.test.ts`, `full-bundle.test.ts` behavior relevant to Markdown options, `markdown.test.ts`, `media-removal.test.ts`, `schema-fallback.test.ts`, `svg-sanitization.test.ts`, and core sanitizer portions of `extractor-xss.test.ts`.
- Core crate adaptation targets: `no-mutate-live-doc.test.ts`, translated into Rust ownership/clone-safety tests rather than live browser document tests.
- CLI adaptation targets: `cli.test.ts`, translated to the HTML-only `lexopt` command surface. URL-fetch and `--user-agent` expectations are replaced by URL-as-source rejection and `--url` metadata-context tests.
- Deferred application/helper tests: `fetch.test.ts`, `proxy.test.ts`, and `perf.test.ts`.
- Separate extractor-crate candidates: `youtube-transcript.test.ts`, `youtube-videodata.test.ts`, `x-oembed-surrogates.test.ts`, `x-article-surrogates.test.ts`, `reddit-author.test.ts`, and `conversation-linkedom.test.ts`, but only when the corresponding extractor is explicitly accepted into scope.
- Explicitly excluded: `bilibili-transcript.test.ts`.

The implementation order should port fixture infrastructure first, then focused tests as each subsystem is implemented. CLI tests are deferred until the Rust library behavior is stable.

## Post-Core Tranche: Separate Extractor and Transcript Library

Deliverable: a separate Rust library for extensible media/site extractors, starting with the YouTube transcript extractor and generic transcript formatting utilities. This must not be coupled to the main Defuddle article-extraction crate.

Rationale:
- `../defuddle/src/utils/transcript.ts` is already generic: it formats timestamped transcript segments and optional chapters into stable HTML and text.
- `../defuddle/src/extractors/_base.ts` provides a clean extractor contract: sync extraction, optional async extraction, `prefersAsync`, caller-provided fetch, shared options, and standardized `ExtractorResult`.
- `../defuddle/src/extractors/youtube.ts` is a strong first consumer because it exercises the design thoroughly: DOM transcript extraction, caption-track language selection, host-fetch fallbacks, transcript XML parsing, chapter extraction, video metadata extraction, and readable transcript grouping.

Implementation scope:
- Create a separate crate or workspace member, tentatively `defuddle-extractors`, that depends on the shared DOM/testing utilities only through explicit interfaces.
- Define extractor traits independent of the main library:
  - `Extractor` with `can_extract` and `extract`.
  - `AsyncExtractor` or async trait methods gated behind an async feature.
  - `prefers_async` for extractors whose async path is strictly better.
  - caller-provided fetch abstraction for deterministic tests and host integration, with no bundled HTTP client.
- Define shared extractor types:
  - `ExtractorOptions` with `language`, `include_replies`, and optional host fetch handle.
  - `ExtractorResult` with `content`, `content_html`, optional `content_selector`, `extracted_content`, and `variables`.
  - Registry support that can be used by applications without requiring the article parser.
- Port transcript utilities first:
  - `TranscriptSegment`,
  - `TranscriptChapter`,
  - `TranscriptResult`,
  - timestamp formatting,
  - transcript HTML/text rendering with escaping and chapter insertion.
- Port YouTube in layers:
  - video id parsing for `youtube.com/watch`, `youtu.be`, and `/shorts/`.
  - `VideoObject`/OpenGraph/video metadata extraction, including stale SPA-data validation.
  - channel-name extraction from DOM, microdata, and player response.
  - caption-track extraction and language preference rules.
  - transcript XML parsing for both srv3 `<p>/<s>` and simple `<text>` formats.
  - transcript grouping by sentence, speaker markers, sparse windows, CJK punctuation, short standalone utterances, and long-turn merging.
  - DOM transcript panel parsing for already-rendered desktop/mobile markup.
  - InnerTube player/next request construction and caption XML parsing behind mocked or host-provided fetch.
- Do not port browser-only transcript-panel opening as a required Rust capability. Runtime clicking, `MutationObserver`, and waiting for YouTube to render new DOM are browser integration concerns. Represent them as an optional host integration hook only after the deterministic extractor passes.

Acceptance gate:
- The extractor crate builds and tests independently from the main article extraction crate.
- All transcript utility tests pass before YouTube extraction work begins.
- Port the upstream YouTube-focused tests from:
  - `../defuddle/tests/youtube-transcript.test.ts`,
  - `../defuddle/tests/youtube-videodata.test.ts`.
- All deterministic tests from those files pass with mocked fetch and static HTML input.
- Tests that require DOM clicking, `MutationObserver`, browser timers tied to live rendering, bundled HTTP behavior, or live network are listed in the exception manifest with a browser-integration reason.
- The main Defuddle crate can remain fully usable without compiling async/network extractor code.
