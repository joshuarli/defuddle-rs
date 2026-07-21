# Cleanup the Kitchen Sink

This document is a systematic audit of code quality issues in the `defuddle-rs` processing pipeline. The goal is to make the codebase feel like a coherent system before any larger architectural restructuring.

## Goal

Systematically clean up the defuddle-rs codebase by executing all 10 phases
in order, using the detailed checklist below for each phase.

Acceptance gate after every phase (each task within a phase):
    cargo test -p defuddle

This runs ALL tests for the core crate: unit tests, integration tests, and
the ~200 upstream fixture parity tests. All must pass.

Phases are ordered by risk. Execute them sequentially — do not skip ahead.
After completing each phase, commit the changes with a message like:
    cleanup: Phase N — <brief description>

If a phase item is blocked (e.g. removing something would break a fixture),
note it, skip it, and continue. Do not block progress on a single item.

After completing all phases, file any items that were deliberately deferred
as a new section at the bottom called "Deferred Items".


## Execution Strategy

**Phase order:** 1 → 2 → 3 → 4 → 6 → 5 → 7 → 10 → 8 → 9

Phases 1-4 are mechanical improvements that can be verified by the existing 200-fixture test suite. Phase 5 (article-specific hacks) changes behavior and needs careful fixture review. Phase 6 (split modules) is a code move — no behavior change. Phases 7-8 are the riskiest refactors and should come after the codebase is clean. Phase 9 (error handling) is API-breaking and should come last unless it blocks a dependency.

Each phase should be a separate commit. Before committing, run `cargo test -p defuddle`.


## Acceptance Gate

Every stage must pass the core crate's full test suite:

```bash
cargo test -p defuddle
```

This runs:
- Unit tests (inline `#[cfg(test)] mod tests` blocks)
- Integration tests (`tests/integration.rs`, `tests/metadata.rs`)
- The **~200 upstream fixture parity tests** (`tests/fixtures.rs`)

If `cargo test -p defuddle` fails, do not commit. Fix the issue before proceeding.

---

## 1. Structural Problems

### 1.1 Article-Specific Hacks in Core Pipeline

The `parse_internal_once` function in `lib.rs` contains literal article title checks that gate behavior for individual pages:

| Location | Title Check | What It Does |
|---|---|---|
| `lib.rs:411` | `"Building Frontend UIs with Codex and Figma"` | Skips `remove_by_selector`, `remove_hidden_elements`, `score_and_remove`, `remove_content_patterns` |
| `lib.rs:427` | `"Node.js and CPU profiling"` (prefix) | Skips `score_and_remove`, `remove_content_patterns`, `remove_metadata_blocks` |
| `lib.rs:466` | `"Numeric Anchor-ID Footnote Pattern"` | 6 raw `.replace()` calls on output HTML |
| `lib.rs:478` | Hardcoded Vox Wikipedia link escape | `.replace("Vox_\\(website) ", "Vox_\\(website\\) ")` |
| `standardize.rs:13,53,65` | `"Building Frontend UIs with Codex and Figma"` | Enables/disables specific standardization steps |
| `standardize.rs:17` | `"Node.js and CPU profiling"` (prefix) | Skips `standardize_drop_caps` |
| `standardize.rs:91-92` | `"Yarden Katz"` / `"Product Manager, Figma"` | Hardcoded author removal |
| `standardize.rs:122` | `"To learn more about the Figma MCP server"` | Footer trimming |

**Problem:** These are fixture-specific band-aids baked into the generic pipeline. They make the codebase untestable in isolation and create coupling between the pipeline orchestration and individual webpage layouts.

### 1.2 Dead Code Masked by `#[allow(dead_code)]`

The codebase has 4 module-level `#![allow(dead_code)]` attributes and 5 function-level `#[allow(dead_code)]` attributes that suppress legitimate warnings:

| Item | File | Why Dead |
|---|---|---|
| `remove_by_selector_protected` | `pipeline.rs:266` | ~90% duplicated from `remove_by_selector`, never called |
| `remove_small_images` | `pipeline.rs:498` | Empty body — stub |
| `convert_callouts_to_markdown` | `markdown.rs:1283` | Replaced by DOM-level handler |
| `strip_keep_inline_tags` | `markdown.rs:1336` | Only called from dead function above |
| `is_likely_content` | `scoring.rs:169` | Never called |
| `ExtractorResult::content_selector` | `extractor.rs:8` | Always set to `None` |
| `GitHubExtractor::number` | `extractor.rs:846` | Set but never read |
| `BLOCK_ELEMENTS_SELECTOR` | `constants.rs:32` | Unused |
| `HIDDEN_EXACT_SKIP_SELECTOR` | `constants.rs:35` | Unused |
| `HIDDEN_EXACT_SELECTOR` | `constants.rs:37` | Unused |
| `FOOTNOTE_INLINE_REFERENCES_SELECTOR` | `constants.rs:738` | Only referenced in a comment |
| `_post_id`, `_comment_id`, `_lang`, `_tag` etc. | Various | Computed but never used |

### 1.3 Duplicated Functions Across Modules

Several utility functions are copy-pasted across files:

- **`has_closest`**: `pipeline.rs:12`, `math.rs:218`, `footnotes.rs:846` — identical 3-line implementation
- **`next_element_sibling`**: `pipeline.rs:30`, `metadata.rs:629` — identical
- **`prev_element_sibling`**: `pipeline.rs:19`, `metadata.rs:615` — identical
- **`escape_html`**: `utils.rs:183` (public), `extractor.rs:122` (private) — same logic
- **`html_escape`**: `standardize.rs:2125` — near-duplicate of `escape_html`
- **`extract_hostname` / `parse_domain`**: `extractor.rs:81`, `metadata.rs:113` — near-duplicates
- **`remove_by_selector` / `remove_by_selector_protected`**: `pipeline.rs:41,266` — ~90% shared logic

### 1.4 Monolithic Modules

- **`extractor.rs`** (2635 lines): 8 extractor implementations crammed into a single file. Each should be a separate file under `src/extractors/`.
- **`standardize.rs`** (2235 lines): 30+ standardization steps, many with deep nesting and long bodies. Could be split into category-specific files.

### 1.5 Excessively Large Parameter Lists

`parse_internal` (14 parameters) and `parse_internal_once` (13 parameters) are both marked `#[allow(clippy::too_many_arguments)]`. The retry loop in `parse_internal` re-parses the original HTML up to 4 times just to pass different boolean flags.

---

## 2. Allocation Churn

### 2.1 Unnecessary `.to_lowercase()` on Tag Names

There are **38** call sites across `pipeline.rs`, `standardize.rs`, `metadata.rs`, `scoring.rs`, and `footnotes.rs` that call `doc.tag_name(id).to_lowercase()`. Every call allocates a new `String`. In many cases, the tag is immediately matched against static strings (e.g. `tag == "pre" || tag == "code"`), which could be done case-insensitively without allocation.

### 2.2 Unnecessary `.to_vec()` on Children

There are **42** calls to `doc.children(node).to_vec()` which clones the entire children slice into a new heap-allocated `Vec<NodeId>`. Most are necessary to avoid borrow-checker issues when mutating during iteration, but some in `standardize.rs` could be restructured.

### 2.3 Serialize-Then-Re-parse Round-Trips

These patterns serialize DOM subtrees to HTML strings and immediately parse them back:

| Pattern | Location | Severity |
|---|---|---|
| `outer_html` → `replace_with_html` | `standardize.rs:1867-1868` | Serializes a `<pre>`, then re-parses to graft into `<code>` |
| `outer_html` → `replace_with_html` | `standardize.rs:1478` | Serializes child, re-parses into table |
| `inner_html` → `Document::parse` → text extract | `standardize.rs:1626-1630` | Serializes code block HTML, re-parses as standalone doc just to get text |
| `remove_iframes_from_result` | `sanitize.rs:121-123` | Full parse → modify → serialize on the already-computed result |
| Child-by-child `outer_html` → concatenate → `replace_with_html` | `standardize.rs:1432-1439` | Builds HTML string from children then re-parses the blob |

### 2.4 Retry Loop Re-Parses Original HTML

`parse_internal` calls `Document::parse(html)` up to **4 times** on the same input, each time running html5ever's tokenizer and tree builder from scratch.

### 2.5 Repeated `get_attr` Calls on Same Node

- `standardize.rs:2159-2173`: `get_attr(child, "href")` called **3 times** on the same node
- `standardize.rs:1291,1296`: `get_attr(id, "data-as")` called twice (first for `.is_some()`, second to get value)
- `standardize.rs:477,491`: `get_attr(id, "src")` called twice when the first result is still in scope

### 2.6 Unnecessary String Conversions

There are **320** `.to_string()` calls across the crate. Many are on owned Strings or are immediately discarded. Examples:
- `pipeline.rs:135-155`: `class_val` is lowercased, then its clone is re-lowercased
- `metadata.rs:318-326`: `site_name` normalized to lowercase 3 separate times
- `standardize.rs:1613-1614`: result of `to_lowercase()` is lowercased again
- `dom.rs:790,801,812`: `.to_lowercase()` called on both `val` and `aval` in tight comparison loops

---

## 3. Regex Abuse

### 3.1 Inline Compilations (Not Cached)

There are **~50** regex compilations that happen at runtime on every invocation instead of being cached in `OnceLock`/`LazyLock` statics:

| File | Count | Context |
|---|---|---|
| `standardize.rs` | ~12 | Inline compilations in `recover_noscript_images`, `normalize_codemirror_blocks`, `standardize_code_blocks`, `clean_code_html_for_text`, `code_text_from_html` |
| `markdown.rs` | ~38 | Inline compilations in `postprocess_markdown`, `preprocess_html_for_markdown`, `convert_callouts_to_markdown`, `resolve_best_image_sources`, etc. |
| `pipeline.rs` | ~6 | Inline compilations in `remove_content_patterns`, `remove_metadata_blocks` |
| `footnotes.rs` | ~3 | Inline compilations in `standardize_footnotes`, `collect_pulldown_cmark_defs`, `collect_heading_separated_defs` |
| `metadata.rs` | ~4 | Inline compilations in `try_separator_split`, `parse_date_text`, `clean_author_string` |
| `extractor.rs` | ~6 | Inline compilations in github, chatgpt, substack, twitter extractors |

The `markdown.rs` `postprocess_markdown` function alone compiles ~15 regexes per conversion call.

### 3.2 HTML Parsing with Regex

Several functions parse HTML strings with regex instead of using the DOM:

- `markdown.rs:541-580` (`preprocess_html_for_markdown`): Entire pre-processing chain operates on HTML as a string
- `markdown.rs:1282-1333` (`convert_callouts_to_markdown`): Regex-based HTML parsing of callout blocks
- `markdown.rs:1417-1442` (`resolve_best_image_sources`): Regex on HTML to parse `<img>` tags
- `markdown.rs:1452-1468` (`convert_code_block_language`): Regex on HTML
- `markdown.rs:1470-1499` (`convert_embeds_to_links`): Regex on HTML to process `<iframe>` tags
- `standardize.rs:386-420` (`recover_noscript_images`): Regex on inner HTML to extract `<img>` attributes
- `standardize.rs:1581-1616` (`normalize_codemirror_blocks`): Regex on inner HTML
- `standardize.rs:2071-2097`: Regex on HTML to strip tags for text extraction
- `extractor.rs:928-972` (GitHub): String replace and regex on inner HTML for table/code-block manipulation
- `extractor.rs:1544-1582` (Substack): 6 regex replacements on raw HTML (button-wrapper, subscribe widgets, etc.)

### 3.3 `fix_inline_spacing_line` Design

`markdown.rs:fix_inline_spacing_line` applies 30 regexes sequentially to each line. For each regex it calls `is_match` + `replace_all`, which means two full scans per regex per line. The regexes are cached in `OnceLock` (commit `5b823d7` improved this), but the algorithm is still O(n × m) where n = regexes and m = line length.

---

## 4. Error Handling

### 4.1 Raw `.unwrap()` Calls

There are **94** `.unwrap()` calls. Most are on `Regex::new(...)` with static patterns (safe in practice but not guaranteed). The genuinely dangerous ones:

| Location | Risk |
|---|---|
| `pipeline.rs:585` | `main_content.unwrap()` after `?` check — fragile |
| `pipeline.rs:1375,1380` | `previous.unwrap()`, `next.unwrap()` after `is_some_and` — fragile |
| `metadata.rs:1064,1070` | `author.chars().next().unwrap()` after `is_empty` check — fragile |
| `extractor.rs:962,1240,1276,1548,1563` | Regex `new(...).unwrap()` on non-trivial patterns |
| `markdown.rs:1257,1346,1438,1465,1505` | `caps.get(N).unwrap()` — will panic on malformed input |

### 4.2 Extract Path Error Suppression

Extractor modules silently return empty strings or defaults on errors. None propagate errors upward. This is the upstream TypeScript design (no error path), but it means the Rust port has no way to signal extraction failures.

---

## 5. API / Architecture Issues

### 5.1 `prune.rs` Is Stubbed Out

The `prune` module (27 lines) has a clean `Registry` + `fn(&mut Document)` design but only registers the Substack extractor's prune handlers. The infrastructure exists for site-specific pre-pipeline DOM cleanup, but it's underused — other extractors still do cleanup inline during `extract()`.

### 5.2 `remove_iframes_from_result` Double-Dips

`lib.rs:208-209` removes iframes from extractor results by re-parsing the HTML string. Then `lib.rs:278-280` also calls `remove_iframes_from_result` on the final result. Both serialize, parse, modify, and re-serialize.

### 5.3 `remove_small_images` Is Empty

`pipeline.rs:498` is a stub. Its name appears in `DefuddleOptions` and the upstream API, but it does nothing in the Rust port.

### 5.4 `ExtractorResult::content_selector` Is Never Used

The `content_selector` field is always `None`. The TypeScript upstream sets it from site-specific CSS selectors, but the Rust port never populates it.

### 5.5 Selector Parser Is Buggy

The hand-rolled CSS selector parser in `dom.rs:909-1188` is 279 lines of state machine with acknowledged bugs for `:not()` attribute negation. The workaround in `pipeline.rs:65-96` manually handles `script`, `iframe`, `video`, and `audio` :not() selectors. The parser also rejects combinators (`>`, `+`, `~`) and `:has()`.

---

## 6. Code Style Inconsistencies

### 6.1 Dead Expressions

- `pipeline.rs:217`: `class_val.contains("author-bio");` — standalone expression, result discarded
- `pipeline.rs:1095`: `let _ = parent;` — binding immediately dropped
- `standardize.rs:1513`: `let _tag = doc.tag_name(code_id).to_string();` — computed, never read
- `markdown.rs:309`: `let url = String::new();` — never written; the `!url.is_empty()` check at line 381 is always false

### 6.2 Duplicated String Patterns Across Extractors

Every extractor individually:
- Builds `FxHashMap::from_iter([("title",...), ("author",...), ...])` for variables
- Calls `.first().map(|id| doc.text_content(*id).trim().to_string()).unwrap_or_default()` (40+ occurrences)
- Splits ISO dates with `date.split('T').next().unwrap_or("").to_string()` (7 occurrences)
- Does URL scheme checks with `starts_with("http://") || starts_with("https://")`

---

## Cleanup Roadmap

Phases 1-3 are done.

### Phase 4: Reduce Allocation Churn (medium risk, requires careful review)

- [ ] **4.1** Add a `tag_name_is(id, name) -> bool` case-insensitive comparison method to `Document` to eliminate `tag_name().to_lowercase()` allocations. Use it at all 38 call sites.
- [ ] **4.2** Add a `get_attr_lower(id, name) -> Option<Cow<str>>` or just pre-lowercase the attr name parameter. Fix the 3× repeated `get_attr(child, "href")` call at `standardize.rs:2159-2173`.
- [ ] **4.3** Fix the doubled `get_attr` calls: `standardize.rs:1291+1296` (data-as), `standardize.rs:477+491` (src).
- [ ] **4.4** Replace serialize-then-reparse round-trips with DOM-level node transfers:
  - `standardize.rs:1867-1868`: Move `<pre>` children into `<code>` via DOM
  - `standardize.rs:1478`: Transfer child nodes into table via DOM
  - `standardize.rs:1626-1630`: Extract text from DOM directly instead of via HTML serialize → parse → text_content
  - `standardize.rs:1432-1439`: Build replacement via DOM operations instead of string concatenation
- [ ] **4.5** Add a `remove_iframes_from_doc` step to the standard pipeline in `parse_internal_once` instead of doing it post-hoc via HTML re-parse. Remove the double `remove_iframes_from_result` call pattern.
- [ ] **4.6** Eliminate redundant `.to_string()` calls where a `&str` or `Cow<str>` would suffice. Target the 320 identified sites, starting with the most egregious:
  - `metadata.rs:318-326` (triple lowercasing of site_name)
  - `standardize.rs:1613-1614` (double lowercasing)
  - `dom.rs:790,801,812` (lowercasing both sides of a comparison)

### Phase 5: Eliminate Article-Specific Hacks (medium-high risk, affects fixtures)

- [ ] **5.1** Create a `PageOverrides` struct with fields for `skip_remove_selectors`, `skip_hidden_removal`, `skip_scoring`, `skip_content_patterns`, `skip_metadata_blocks`, `custom_html_postprocess`, `custom_markdown_postprocess`, `standardize_exceptions`.
- [ ] **5.2** Define overrides for the Figma article, Lazy Profiling article, and Numeric Anchor-ID Footnote article in a lookup (either inline in the overrides system or in `tests/` as test-only fixtures).
- [ ] **5.3** Remove all hardcoded title checks from `lib.rs`, `standardize.rs`, `pipeline.rs`.
- [ ] **5.4** Move the `standardize_figma_header`, `trim_figma_footer`, and lazy-profile-specific handling into the override definitions.
- [ ] **5.5** Move the Substack `cleanup_body_html` regex chain to use the `prune` module (DOM-level removal) for patterns where DOM matching works, falling back to string manipulation only for the patterns that require regex over serialized HTML.

### Phase 6: Split Monolithic Modules (low risk, structural only)

- [ ] **6.1** Split `extractor.rs` into `src/extractors/`:
  - `mod.rs` — trait, Registry, comment builder
  - `hackernews.rs`, `reddit.rs`, `github.rs`, `chatgpt.rs`, `substack.rs`, `mastodon.rs`, `twitter.rs`
- [ ] **6.2** Split `standardize.rs` into `src/standardize/`:
  - `mod.rs` — `standardize_content` orchestrator
  - `headings.rs`, `tables.rs`, `code_blocks.rs`, `images.rs`, `elements.rs`, `whitespace.rs`, `urls.rs`

### Phase 7: Refactor Pipeline Orchestration (higher risk)

- [ ] **7.1** Create a `PipelineConfig` struct to replace the 14-parameter `parse_internal` / 13-parameter `parse_internal_once` signatures:
  ```rust
  struct PipelineConfig {
      remove_exact: bool,
      remove_partial: bool,
      remove_hidden: bool,
      remove_iframes: bool,
      do_standardize: bool,
      separate_markdown: bool,
      content_selector: Option<String>,
      debug: bool,
  }
  ```
- [ ] **7.2** Move the retry loop from `parse_internal` into a simpler construct. Instead of re-parsing HTML 4 times, clone the cleaned `Document` at the point where options diverge and re-run only the affected pipeline steps. Alternatively, accept the simplicity of re-parse and document the trade-off.
- [ ] **7.3** Replace raw `.unwrap()` calls with proper error handling. At minimum, replace `pipeline.rs:585`, `pipeline.rs:1375,1380`, `metadata.rs:1064,1070`, and the `caps.get(N).unwrap()` calls in `markdown.rs` with `.ok_or(...)` or `match` arms.

### Phase 8: Fix the Selector Parser (high risk, complex)

- [ ] **8.1** Either fix the `:not()` selector matching in `SimpleSelector::parse` or document the full set of known-buggy patterns and ensure all are worked around.
- [ ] **8.2** Consider replacing the hand-rolled parser with `cssparser` (already in the dependency tree via html5ever) to eliminate the maintenance burden of a custom selector engine.
- [ ] **8.3** If keeping the hand-rolled parser, add comprehensive unit tests for each selector type: tag, `#id`, `.class`, `[attr]`, `[attr=val]`, `[attr*=val]`, `[attr^=val]`, `[attr$=val]`, `[attr~=val]`, `:not()`, `i` flag.

### Phase 9: Improve Error Handling (medium risk)

- [ ] **9.1** Add an `Error` variant to `ExtractorResult` or make `extract()` return `Result<ExtractorResult, Error>` so extractors can signal failures.
- [ ] **9.2** Replace all regex `new(...).unwrap()` calls with `.expect("valid static regex pattern: ...")` with a descriptive message, or use a compile-time regex library if possible.
- [ ] **9.3** Add `#[track_caller]` to DOM methods that can panic on invalid `NodeId` to improve debuggability.

### Phase 10: Polish and Consistency (low risk)

- [ ] **10.1** Remove `#[allow(dead_code)]` on `ENTRY_POINT_ELEMENTS` and `BLOCK_ELEMENTS` in constants.rs (both are actually used)
- [ ] **10.2** Audit all `#[allow(clippy::too_many_arguments)]` sites — these should shrink naturally after Phase 7
- [ ] **10.3** Standardize the `LazyLock` import pattern — some files use `std::sync::LazyLock`, others use `std::sync::OnceLock`. Pick one.
- [ ] **10.4** Run `cargo clippy --all-targets` and fix all actionable warnings
- [ ] **10.5** Add a CI check that fails on `#[allow(dead_code)]` and `#[allow(unused)]` unless accompanied by a comment explaining why

