# Defuddle (Rust Port) — Agent Reference

## What This Is

A Rust port of [Defuddle](https://github.com/kepano/defuddle), a TypeScript library that extracts clean, readable article content from HTML. This crate accepts static HTML strings and returns extracted content (HTML or Markdown) plus metadata. No network, no browser, no JavaScript runtime.

The TypeScript reference lives at `../defuddle` (commit `6c39f09`). Upstream test fixtures and expected output are vendored in `tests/upstream/`.

## Workspace

```
defuddle-rs/
├── Cargo.toml              # workspace root
├── PLAN.md                 # original port plan (can be deleted once absorbed into this doc)
├── docs/dependencies.md    # dependency rationale
├── tests/
│   ├── upstream/           # vendored upstream fixtures and expected output
│   │   ├── SNAPSHOT.md     # source commit reference
│   │   ├── defuddle/fixtures/   # 202 .html input files
│   │   └── defuddle/expected/   # 202 .md + 3 .html golden files
│   └── upstream-exceptions.toml  # tests/fixtures excluded from current gate
├── crates/
│   ├── defuddle/           # core library — HTML in, content out
│   │   ├── src/
│   │   │   ├── lib.rs           # public API, DefuddleOptions, DefuddleResult, parse_html
│   │   │   ├── dom.rs           # mutable DOM adapter over html5ever/rcdom
│   │   │   ├── constants.rs     # CSS selectors, blocked patterns, element lists
│   │   │   ├── utils.rs         # count_words (CJK-aware), normalize_text, entity decode/encode
│   │   │   ├── sanitize.rs      # strip_unsafe_elements, is_dangerous_url
│   │   │   ├── schema.rs        # extract_schema_org_data, collect_meta_tags
│   │   │   ├── metadata.rs      # extract title, author, site, date, favicon, image, language
│   │   │   ├── scoring.rs       # score_element, find_main_content, is_likely_content
│   │   │   ├── pipeline.rs      # remove_by_selector, remove_hidden_elements, extract_content
│   │   │   ├── standardize.rs   # standardize_content (comment removal, layout tables, empty elements)
│   │   │   ├── markdown.rs      # html_to_markdown, content_to_markdown (wraps html-to-markdown-rs)
│   │   │   └── extractor.rs     # Extractor trait, Registry, 8 site extractors, comment builder, BBCode
│   │   └── tests/
│   │       ├── fixtures.rs      # golden fixture runner (compares against tests/upstream/expected/)
│   │       ├── integration.rs   # focused parity tests (markdown, xss, media, schema fallback, debug, etc.)
│   │       └── metadata.rs      # metadata extraction unit tests
│   └── defuddle-cli/       # thin lexopt binary
│       ├── src/main.rs
│       └── tests/cli.rs         # stdin, file, markdown, json, frontmatter, property, output tests
```

## Data Flow

```
parse_html(html: &str, options: DefuddleOptions) -> DefuddleResult
│
├─ 1. Parse HTML → Document (html5ever → custom DOM adapter)
├─ 2. Extract metadata (title, author, site, date, etc.) from original doc
├─ 3. Extract schema.org JSON-LD and meta tags
├─ 4. Check extractor registry — if URL matches a site extractor:
│     └─ Run extractor.extract() → ExtractorResult (content_html + variables)
│        Skip standard pipeline, build DefuddleResult from extractor output
├─ 5. (Standard pipeline) Find main content element via entry-point scoring
├─ 6. Remove clutter: exact/partial selectors, hidden elements
├─ 7. Standardize HTML: strip comments, flatten layout tables, remove empty elements
├─ 8. Sanitize: strip dangerous elements (script/style/noscript/object/embed/applet/base)
│     and dangerous attributes (on*, srcdoc, javascript: URIs)
├─ 9. Optionally convert to Markdown via html-to-markdown-rs
└─ 10. Return DefuddleResult
```

### DOM Adapter (`dom.rs`)

Custom mutable tree built on top of `html5ever` + `markup5ever_rcdom`. Each node is a `NodeId(usize)` index into a flat `Vec<Node>`.

- **Node types**: Document, Doctype, Element (name + attrs), Text, Comment
- **Key operations**: `parse`, `query_selector_all`, `closest`, `contains`, `text_content`, `get_attr`/`set_attr`/`remove_attr`, `remove`, `unwrap`, `replace_with_html`, `append_html`, `inner_html`, `outer_html`, `clone`
- **Selector parser**: Handles tag, `#id`, `.class`, `[attr]`, `[attr=val]`, `[attr*=val]`, `[attr^=val]`, `[attr$=val]`, `[attr~=val]`, `:not()`. Case-insensitive flag (`i`) supported. Combinators (`>`, `+`, `~`) and `:has()` are rejected.
- **Known limitations**: `:not()` selector matching is buggy for attribute-negation (e.g. `:not([src])`). Worked around in `pipeline.rs` by handling `script`/`iframe`/`video`/`audio` :not() selectors manually.

### Extractor Registry (`extractor.rs`)

URL-pattern matching against a flat list of (domain, factory) pairs. Eight extractors implemented:

| Extractor | Domains | Features |
|---|---|---|
| HackerNews | news.ycombinator.com | Post + nested comments, listing page detection |
| Reddit | old.reddit.com | Old-reddit comment tree, post body |
| GitHub | github.com | Issue/PR body extraction, repo info |
| ChatGPT | chatgpt.com | Conversation turns with heading attribution |
| Substack | substack.com | Notes via ProseMirror, custom-domain detection |
| Mastodon | mastodon.* | Post + image + replies, self-reply thread detection |
| BBCode Data | store.example.com | Steam-style BBCode → HTML conversion |
| Twitter/X | — | Not implemented (requires async or complex DOM paths) |

All extractors are DOM-only (no network). No async extractors (YouTube, Bilibili) are implemented. The `Extractor` trait has `can_extract(&self, doc: &Document) -> bool` and `extract(&self, doc: &Document) -> ExtractorResult`.

### Site Extractors NOT Implemented

The following upstream extractors are not ported:
- Twitter/X (requires async + complex DOM)
- YouTube (requires async + transcript API)
- Bilibili (explicitly excluded)
- Claude, Gemini, Grok, Medium, Wikipedia, LinkedIn, Threads, Bluesky, Discourse, LeetCode, LWN, C2 Wiki, NYTimes
- X Oembed, X Article surrogates

These are listed in `tests/upstream-exceptions.toml` as `site_extractor_out_of_scope`.

## Dependencies

```
defuddle (core):
  html5ever, markup5ever_rcdom  — HTML parsing
  regex                          — BBCode conversion, metadata date parsing
  serde, serde_json              — schema.org JSON-LD, serialization
  thiserror                      — error type derivation
  html-to-markdown-rs            — HTML → Markdown conversion

defuddle-cli:
  lexopt                         — argument parsing (no clap, no commander)
  atty                           — TTY detection for stdin
  serde_json                     — JSON output
```

No HTTP client, no TLS, no proxy, no async runtime, no browser automation, no JavaScript engine. Verified via `cargo tree -p defuddle`.

## Test Suite

```
56 tests total (5 suites, 0 failures)

lib (13):        dom adapter spike, utils (5), sanitizer (3), markdown (3)
fixtures (3):    harness discovery, exception manifest validation, golden runner
integration (23): markdown rules, media removal, schema fallback, svg/xss sanitization,
                  debug, full-bundle, no-mutate, selector/property/content tests,
                  javascript/dangerous-uri stripping, event handler stripping
metadata (8):    og:title, author byline, schema.org, language, favicon, date, preamble parity
cli (8):         stdin, file, markdown, json, frontmatter, property, URL rejection, output file
doctest (1):     crate documentation example
```

The golden fixture runner compares all 202 fixtures against vendored expected output but currently reports 0 matches — the pipeline produces different HTML/Markdown than the TypeScript reference. This is due to incomplete standardization and Markdown rule ports. No tests panic or crash.

## Key Design Decisions

- **Own DOM adapter** over `html5ever` rather than using `scraper` or `lol_html`. Reason: Defuddle needs whole-document mutation, subtree comparison, and selected-element serialization, which streaming rewriters can't provide.
- **`html-to-markdown-rs`** for Markdown conversion rather than a bespoke renderer. Custom rules (footnotes, callouts, math, tables) are layered through HTML preprocessing in `standardize.rs`.
- **No async** in the core crate. Async extractors (YouTube, Reddit fallback) would go in a separate `defuddle-extractors` crate with caller-provided fetch abstraction.
- **`lexopt`** for CLI rather than `clap`. Keeps the dependency tree minimal.

## How To…

### Run Tests
```bash
cargo test                          # all tests
cargo test --test fixtures          # golden fixture runner only
cargo test --test integration       # focused parity tests
cargo test -- --nocapture           # show eprintln output
```

### Debug a Fixture
```bash
cargo test --test fixtures -- --nocapture 2>&1 | grep -A 20 "EXTRACTOR comments--"
```

### Add a New Extractor
1. Add a `pub mod` in `extractor.rs` with a `create(doc, url) -> Option<Box<dyn Extractor>>` factory
2. Implement `Extractor` trait (`can_extract`, `extract`)
3. Register it in `build_registry()`
4. Add tests in `tests/integration.rs`

### Add a New Standardization Rule
1. Add a function in `standardize.rs`
2. Call it from `standardize_content()`
3. Tests in `tests/integration.rs`

## Known Gaps

- **Fixture parity**: 0/202 golden fixtures match byte-for-byte. Full parity requires:
  - Complete port of `src/markdown.ts` custom rules (exclamation-mark spacing, ragged table columns, math conversion, tag-like text escaping, parenthesis escaping in links)
  - Complete port of `src/standardize.ts` (headings, code blocks, images, callouts, footnotes, math, permalink anchors, attribute stripping, wrapper flattening, br normalization)
- **Retry logic**: The upstream parse() retries with reduced aggressiveness when word count is low. Not ported.
- **Schema text fallback**: When schema.org text is 1.5x longer than extracted content, re-extract from the schema-matched element. Not ported.
- **Mobile styles, shadow DOM, React SSR**: Browser-only features not ported (out of scope).
- **Performance benchmarks**: Not implemented.
- **Async extractors**: YouTube transcripts, Bilibili (excluded), X oEmbed not ported.
- **:`not()` selectors**: Buggy in the simple CSS selector parser. Worked around manually for script/iframe/video/audio.
