This is a rust port of [Defuddle](https://github.com/kepano/defuddle), a TypeScript library that extracts clean, readable article content from HTML. This crate accepts static HTML strings and returns extracted content (HTML or Markdown) plus metadata. No network, no browser, no JavaScript runtime.

A Rust port of `../defuddle` (commit `6c39f09`), a TypeScript library that extracts clean, readable article content from HTML.

Upstream test fixtures and expected output are vendored in `tests/upstream/`.

## Workspace

```
defuddle-rs/
├── Cargo.toml              # workspace root
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
| Twitter/X | x.com, twitter.com | Static article and conversation extraction |

All extractors are DOM-only (no network). No async extractors (YouTube, Bilibili) are implemented. The `Extractor` trait has `can_extract(&self, doc: &Document) -> bool` and `extract(&self, doc: &Document) -> ExtractorResult`.

### Site Extractors NOT Implemented

The following upstream extractors are not ported:
- YouTube (requires async + transcript API)
- Bilibili (explicitly excluded)
- Claude, Gemini, Grok, Medium, Wikipedia, LinkedIn, Threads, Bluesky, Discourse, LeetCode, LWN, C2 Wiki, NYTimes
- X Oembed, X Article surrogates

These are listed in `tests/upstream-exceptions.toml` as `site_extractor_out_of_scope`.


## Key Design Decisions

- **Own DOM adapter** over `html5ever` rather than using `scraper` or `lol_html`. Reason: Defuddle needs whole-document mutation, subtree comparison, and selected-element serialization, which streaming rewriters can't provide.
- **`html-to-markdown-rs`** for Markdown conversion rather than a bespoke renderer. Custom rules (footnotes, callouts, math, tables) are layered through HTML preprocessing in `standardize.rs`.
- **No async** in the core crate. Async extractors (YouTube, Reddit fallback) would go in a separate `defuddle-extractors` crate with caller-provided fetch abstraction.
- **`lexopt`** for CLI rather than `clap`. Keeps the dependency tree minimal.

## How To…

### Run Tests
```bash
cargo test                          # core and CLI tests (default members)
cargo test --workspace              # include the opt-in WASM adapter
cargo test --test fixtures          # golden fixture runner only
cargo test --test integration       # focused parity tests
cargo test -- --nocapture           # show eprintln output
```

The `defuddle-wasm` crate is a workspace member but not a default member. The `wasm-release` profile is tuned for a small artifact.

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
- **Retry logic**: Low-word-count retry behavior exists, but may still differ from upstream in its aggressiveness choices.
- **Schema text fallback**: Schema text fallback exists, but its selection or Markdown conversion may still differ from upstream.
- **Math parity**: Math extraction and rendered math HTML/MathML/KaTeX/MathJax fidelity are deferred.
- **Mobile styles, shadow DOM, React SSR**: Browser-only features not ported (out of scope).
- **Performance benchmarks**: Not implemented.
- **Async extractors**: YouTube transcripts, Bilibili (excluded), and X oEmbed are not ported.
- **:`not()` selectors**: Buggy in the simple CSS selector parser. Worked around manually for script/iframe/video/audio.
