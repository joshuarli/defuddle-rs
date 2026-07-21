# Defuddle Rust

This repository is a Rust port of [Defuddle](https://github.com/kepano/defuddle), a TypeScript library that extracts readable article content from static HTML. The core crate accepts an HTML string and returns cleaned HTML, optional Markdown, and metadata. It does not use a browser, JavaScript runtime, network access, or asynchronous fetching.

The port tracks the upstream snapshot documented in `tests/upstream/SNAPSHOT.md`. Upstream fixtures and expected output are vendored under `tests/upstream/`.

## Repository layout

```
defuddle-rs/
├── Cargo.toml
├── tests/
│   ├── upstream/
│   │   ├── SNAPSHOT.md
│   │   ├── defuddle/fixtures/   # HTML inputs
│   │   └── defuddle/expected/   # golden HTML/Markdown outputs
│   └── upstream-exceptions.toml # intentionally excluded fixtures
└── crates/
    ├── defuddle/                # core library
    ├── defuddle-cli/            # stdin/file CLI
    └── defuddle-wasm/           # opt-in WASM adapter
```

The core source is organized around the extraction pipeline rather than one large implementation module:

| Module | Responsibility |
|---|---|
| `lib.rs` | Public API, options/results, metadata setup, adapter dispatch, retry loop |
| `dom.rs` | Mutable DOM adapter over `html5ever` and `markup5ever_rcdom` |
| `pipeline.rs` | Main-content selection, pruning, scoring, selector removal, hidden-content removal |
| `prune.rs` | Document-wide pre-extraction cleanup hooks |
| `metadata.rs`, `schema.rs` | Metadata, meta tags, and JSON-LD extraction |
| `standardize/` | DOM normalization after main-content selection |
| `footnotes/` | Footnote reference/definition collection and canonicalization |
| `embeds/` | Platform embed normalization, currently YouTube and Twitter/X iframe handling |
| `extractors/` | Site-specific `SiteAdapter` implementations, DOM output builders, and shared comment helpers |
| `sanitize.rs` | Unsafe element, attribute, and URL removal |
| `markdown.rs` | HTML-to-Markdown conversion and final Markdown normalization |
| `overrides.rs` | Deliberate page-specific exceptions to the standard pipeline |
| `constants.rs`, `selector.rs`, `utils.rs` | Shared selectors, matching primitives, text/entity utilities |

## Data flow

`parse_html(html, options)` follows this high-level sequence:

1. Parse the input into the mutable `Document` tree.
2. Run document-wide prune hooks.
3. Extract schema data, meta tags, and metadata from the original document.
4. Apply requested iframe removal and check the site-adapter list.
5. If an adapter matches, it returns a `ContentResult` containing its output DOM and variables. Site adapters bypass the generic content pipeline.
6. Otherwise, run the standard pipeline:
   - select the main content element, or use the body/content selector;
   - remove selectors, hidden elements, low-scoring blocks, and boilerplate;
   - apply page overrides and resolve URLs;
   - standardize the selected DOM subtree;
   - sanitize unsafe elements, attributes, and URLs;
   - serialize HTML and optionally convert the selected content to Markdown.
7. If the result is unusually short, retry the generic pipeline with progressively less aggressive pruning. Each retry reparses the original HTML.

The standardization sequence lives in `standardize/mod.rs`. Its submodules have narrow responsibilities:

- `callouts/` converts Bootstrap alerts and Hugo admonitions into the internal callout shape: `.callout[data-callout]` with `.callout-title` and `.callout-content` children.
- `elements.rs` handles general element normalization, wrappers, roles, empty elements, definition lists, and preformatted HTML semantics. Renderer-specific code-block heuristics have intentionally been removed.
- `headings.rs` normalizes heading structure and trailing headings.
- `images.rs` recovers lazy/noscript images, normalizes `srcset`, removes placeholders, and deduplicates images.
- `tables.rs` distinguishes data tables from layout tables.
- `urls.rs` resolves relative URLs inside the selected subtree.
- `whitespace.rs` handles comments, spaces, inline whitespace, and significant whitespace preservation.

`footnotes/` is deliberately separate from general element standardization. It collects the supported inline-reference and definition formats, builds a canonical `<div id="footnotes"><ol>...</ol></div>` structure, and replaces references with stable IDs. Markdown rendering of that canonical structure remains in `markdown.rs`.

`embeds/` is deliberately separate from site extraction. It normalizes generic platform embed elements that can be handled from static HTML alone; it does not fetch transcripts or call external APIs.

## DOM adapter

`dom.rs` owns the mutable tree needed by Defuddle. `NodeId` indexes nodes in a flat vector. Supported node types are document, doctype, element, text, and comment.

Important operations include parsing, subtree queries, `closest`, containment, text extraction, attribute mutation, remove/unwrap, HTML replacement/insertion, serialization, and cloning.

The selector parser supports tags, IDs, classes, attribute presence/equality/contains/prefix/suffix/token matching, `:not()`, and the case-insensitive attribute flag. It does not support combinators such as `>`, `+`, or `~`, nor `:has()`. Attribute-negation inside `:not()` has known limitations; targeted workarounds remain in `pipeline.rs` for media and script selectors.

## Site extractors

The adapter selector in `extractors/mod.rs` maps URL patterns and document shapes to synchronous site adapters:

| Extractor | Domains | Scope |
|---|---|---|
| Hacker News | `news.ycombinator.com` | Posts, listings, and nested comments |
| Reddit | `reddit.com`, `old.reddit.com` | Post body and old-Reddit comment trees |
| GitHub | `github.com` | Issues and pull requests |
| ChatGPT | `chatgpt.com` | Conversation turns and citations |
| Substack | `substack.com` and supported custom-domain markup | Notes and ProseMirror content |
| Mastodon | `mastodon.*` | Posts, media, replies, and self-reply threads |
| Twitter/X | `x.com`, `twitter.com` | Static articles and conversations |

All site adapters implement `SiteAdapter::matches` and `SiteAdapter::extract`. They must remain synchronous and DOM-only. Site-specific behavior belongs in the corresponding extractor module; generic embeds, footnotes, sanitization, and standardization should not be added there unless the behavior is genuinely shared.

There are no YouTube transcript or Bilibili adapters. Those require external fetching and are outside the core crate. Other upstream sites not represented by the adapter list are tracked as exclusions in `tests/upstream-exceptions.toml` where relevant.

## Design boundaries

- Keep the DOM adapter because extraction requires whole-tree mutation, subtree comparison, and serialization.
- Keep site-specific extraction separate from generic cleanup. A new website should normally be an extractor, not another branch in `pipeline.rs`, `standardize/`, or `markdown.rs`.
- Keep callouts, footnotes, and embeds as explicit boundaries. Add format-specific behavior to those modules when it is shared across sites.
- Use `html-to-markdown-rs` for the general conversion. HTML preprocessing and postprocessing should only cover Defuddle’s canonical structures and compatibility rules.
- Avoid adding asynchronous or network-dependent behavior to `defuddle`. A future fetch-capable integration belongs in a separate crate with a caller-provided fetch abstraction.
- Prefer the simplest correct implementation and avoid new dependencies unless the existing DOM and utility layers cannot support the behavior.

## Tests and benchmarks

```bash
cargo test                          # default workspace members
cargo test --workspace              # includes the opt-in WASM adapter
cargo test -p defuddle              # core unit, fixture, integration, and metadata tests
cargo test -p defuddle --test fixtures
cargo test -p defuddle --test integration
cargo test -- --nocapture
```

The fixture harness compares the vendored upstream snapshot against expected output and validates the exception manifest. Do not silently update golden output: determine whether a mismatch is a regression, an intentional API difference, or a fixture that is outside the supported scope.

Performance benchmarks and profiling live under `benchmarks/`. New pipeline or adapter work should include representative end-to-end measurements, not only microbenchmarks.

To inspect one fixture while debugging:

```bash
cargo test -p defuddle --test fixtures -- --nocapture 2>&1 | grep -A 20 "EXTRACTOR comments--"
```

## Adding code

To add a site adapter:

1. Add a module under `crates/defuddle/src/extractors/`.
2. Implement `SiteAdapter::matches` and `SiteAdapter::extract`, returning a DOM-backed `SiteExtraction`.
3. Add the adapter factory and name to `find_site_adapter`.
4. Add focused integration coverage and representative fixture coverage when appropriate.

To add a standardization rule:

1. Put it in the narrowest existing `standardize/` submodule. Create a new submodule only when it establishes a meaningful boundary.
2. Call it from `standardize_content()` in the correct order.
3. Preserve non-obvious comments explaining why ordering or exceptions exist.
4. Add focused tests and run the fixture gate.

Do not reintroduce site-specific branches into the generic pipeline, renderer-specific code-block cleanup, BBCode, or math/LaTeX handling without an explicit design decision.

## Repository conventions

- Prefer simple, readable Rust and keep the dependency tree small.
- Do not add banner/separator comments.
- Do not remove useful comments during refactors; update them when behavior changes.
- Use `apply_patch` for source edits.
- Do not run pre-commit hooks.
- Do not commit or push unless explicitly requested. Never push to a remote.
