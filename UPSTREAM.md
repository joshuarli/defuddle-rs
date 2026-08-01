# Upstream synchronization guide

This repository is a behaviorally compatible Rust port of `../defuddle`, not a
line-for-line translation. The TypeScript repository is the reference for
supported extraction behavior; the Rust repository owns its synchronous,
static-HTML implementation and public API.

The vendored fixture snapshot is recorded in `tests/upstream/SNAPSHOT.md`. It
is based on `6c39f0959a2f566469a93a190b50e4b34bcf52c4`; the recent resync work
compared that snapshot with upstream
`6de16345bbc822e69397bd6a126861bad9ff469f`. Fixture changes must be reviewed
separately from source changes: do not update golden output merely because
upstream output changed.

## Architecture map

The two implementations have the same broad phases, but different ownership
and data types at each boundary.

| Upstream Defuddle | Rust port | Porting note |
| --- | --- | --- |
| `src/defuddle.ts::Defuddle.parse()` and `parseInternal()` | `crates/defuddle/src/lib.rs::parse_html()`, `parse_internal()`, and `parse_internal_once()` | The Rust entry point is a free function. It owns parsing, metadata, adapter dispatch, retry policy, and result assembly. |
| `DefuddleOptions` and `DefuddleResponse` in `src/types.ts` | `DefuddleOptions` and `DefuddleResult` in `crates/defuddle/src/lib.rs` | Compare fields before porting an option. Browser, fetch, and async options do not automatically belong in the Rust API. |
| Browser or Linkedom `Document` and `Element` | `crates/defuddle/src/dom.rs::Document` plus `dom::NodeId` | The Rust DOM is a mutable flat node store. Every mutation, subtree query, and serialization step works through node IDs. |
| Derived read-only document facts | `crates/defuddle/src/analysis.rs::AnalysisSnapshot` | The first milestone builds an immutable subtree snapshot with topology, tag membership, text totals, descendant counts, and structural flags. It is profile-gated while parity with the mutable DOM remains the oracle. |
| `src/extractor-registry.ts::ExtractorRegistry` | `crates/defuddle/src/extractors/mod.rs::find_site_adapter()` and `adapter_names()` | Rust adapters are selected by a domain gate followed by `SiteAdapter::matches()`. Keep priority explicit in the candidate array. |
| `BaseExtractor` in `src/extractors/_base.ts` | `extractors::SiteAdapter`, `HtmlExtraction`, and `SiteExtraction` | Rust has no shared extractor constructor. An adapter receives `ExtractionContext` and returns DOM-backed output. |
| `ExtractorResult.content` HTML | `pipeline::ExtractedContent` with a `Document` and root `NodeId` | Use `site_extraction_from_html()` when an adapter naturally builds an HTML string. `extractors/dom_builder.rs::Builder` is preferable for structured output. |
| `standardizeContent()` in `src/standardize.ts` | `standardize::standardize_content()` and `crates/defuddle/src/standardize/` | Preserve the ordering in `standardize/mod.rs`; put a new rule in the narrowest existing submodule. |
| `standardizeExtractorOutput()` in `src/standardize.ts` | `standardize::standardize_extractor_output()` called by `lib.rs::site_result()` | Site-extractor output is a separate boundary from generic page cleanup. |
| `src/removals/*.ts` | `pipeline.rs`, `prune.rs`, `extraction_selectors.rs`, and `scoring.rs` | Upstream removal behavior is spread across several Rust modules. Search for the selector and the behavior phase before choosing a target. |
| `src/elements/footnotes.ts` | `crates/defuddle/src/footnotes/mod.rs::standardize_footnotes()` | Definitions may live outside the selected content root, then become canonical Rust output. |
| `src/elements/callouts.ts` | `crates/defuddle/src/standardize/callouts/` | Callout conversion is an explicit shared boundary. |
| `src/elements/images.ts` | `crates/defuddle/src/standardize/images.rs` | Lazy-image recovery, `srcset`, placeholders, and deduplication are separate Rust operations. |
| `src/elements/headings.ts` | `crates/defuddle/src/standardize/headings.rs` | Heading normalization remains in the standardization sequence. |
| `src/elements/code.ts` | `crates/defuddle/src/standardize/elements.rs` | The Rust port intentionally does not reintroduce renderer-specific code-block heuristics. |
| `src/utils/comments.ts` | `crates/defuddle/src/extractors/comments.rs` | Shared comment markup and nesting serve several site adapters. |
| `src/markdown.ts` | `crates/defuddle/src/markdown.rs` | General conversion uses `html-to-markdown-rs`; Rust preprocessing and postprocessing surround it. |
| `src/metadata.ts` and schema extraction in `defuddle.ts` | `crates/defuddle/src/metadata.rs` and `schema.rs` | Metadata is extracted before source-DOM mutation. |
| `src/utils/dom.ts` URL and unsafe-element handling | `crates/defuddle/src/sanitize.rs` and `standardize/urls.rs` | Keep dangerous URL policy in sanitization; URL resolution is separate normalization. |

## Site extractor map

These upstream extractors have direct Rust counterparts:

| Upstream source | Rust source |
| --- | --- |
| `src/extractors/hackernews.ts` | `extractors/hackernews.rs` |
| `src/extractors/reddit.ts` | `extractors/reddit.rs` |
| `src/extractors/github.ts` | `extractors/github.rs` |
| `src/extractors/chatgpt.ts` | `extractors/chatgpt.rs` |
| `src/extractors/substack.ts` | `extractors/substack.rs` |
| `src/extractors/mastodon.ts` | `extractors/mastodon.rs` |
| `src/extractors/twitter.ts` and the X article/oEmbed paths | `extractors/twitter.rs` |
| `src/extractors/gmail.ts` | `extractors/gmail.rs` |

The Rust adapter registry is intentionally smaller. YouTube and Bilibili
transcripts, Reddit Atom feeds, and other fetch-dependent paths are outside the
core crate because its contract is synchronous, DOM-only, and network-free. Do
not add a network client or async runtime to `defuddle` to close that gap. A
future fetch-capable integration belongs in a separate crate with a
caller-provided fetch abstraction.

## DOM and selector differences

Upstream code can use browser-DOM methods directly. In Rust, corresponding
operations are methods on `Document` and identify nodes with `NodeId`:

| TypeScript operation | Rust operation |
| --- | --- |
| `element.querySelectorAll(selector)` | `doc.query_selector_all_in_subtree(element_id, selector)` |
| `document.querySelectorAll(selector)` | `doc.query_selector_all(selector)` |
| `element.closest(selector)` | `doc.closest(element_id, selector)` |
| `element.innerHTML` / `outerHTML` | `doc.inner_html(element_id)` / `doc.outer_html(element_id)` |
| `element.remove()` | `doc.remove(element_id)` |
| `element.replaceWith(html)` | `doc.replace_with_html(element_id, html)` |
| `element.cloneNode(true)` | Clone the `Document` for an isolated working tree, or use `dom_builder::Builder::clone_children()` for generated output. |

Do not retain a `NodeId` after removing or replacing its ancestor without
checking whether it is still live. When a rule needs document order, sort
collected IDs explicitly: comma-selector results can be grouped by selector
instead of returned in browser document order.

The parser in `crates/defuddle/src/css_selector.rs` supports tags, IDs,
classes, attribute presence/equality/contains/prefix/suffix/token matching,
`:not()`, and the case-insensitive attribute flag. It does not support `>`,
`+`, `~`, or `:has()` as general combinators. For an upstream selector using
those forms:

1. Try a simpler selector plus `closest`, containment, sibling, or attribute
   checks.
2. Add a narrowly scoped manual branch in the owning phase if needed.
3. Extend `css_selector.rs` only when the capability is broadly useful and has
   independent tests.

## Extractor output and options

Upstream extractors return HTML that later passes through
`standardizeExtractorOutput()`. Rust adapters return `SiteExtraction`, and
`lib.rs::site_result()` performs this sequence:

1. `sanitize::strip_unsafe_elements()` removes unsafe output.
2. `standardize::resolve_urls()` resolves URLs against the page URL.
3. `standardize::standardize_extractor_output()` removes source-page
   attributes, classes, empty elements, and spacer artifacts.
4. The result is serialized and optionally converted to Markdown.

Generated extractor classes must be added to
`extractors::comments::is_extractor_class()` if consumers need them to survive
attribute cleanup. Do not run generic page scoring or boilerplate removal over
already extracted site output.

The upstream `BaseExtractor` carries `schemaOrgData`, `includeReplies`,
`language`, and an optional `fetch` function. Rust adapters instead receive the
shared `pipeline::ExtractionContext`, whose stable contract is the parsed DOM,
page URL, metadata, schema data, and `DefuddleOptions`. Before adding a Rust
option, determine whether it changes the public crate contract or is only an
upstream constructor detail. Do not add fetch, transcript retrieval, or async
extraction to the core crate.

The current audit found no additional low-risk behavior to add beyond the
seven resync areas already implemented. Upstream constructor-option forwarding
maps conceptually to `ExtractionContext`, but implementing `includeReplies` or
fetch injection consistently would require a deliberate Rust API decision
across all conversation adapters. Reddit's Atom-feed fallback is likewise
outside the static, network-free core contract. Keep both documented as scope
differences until a separate fetch-capable design exists.

## Resync workflow

For an upstream range, use the source commit recorded in
`tests/upstream/SNAPSHOT.md` as the starting point:

```bash
git -C ../defuddle log --oneline SNAPSHOT..TARGET
git -C ../defuddle diff SNAPSHOT..TARGET -- src tests
```

Process each upstream commit as a classification exercise:

1. Identify the behavior change and its upstream tests or fixtures.
2. Find the Rust phase that owns it using this map and exact symbol search.
3. Classify it as a direct port, DOM/API adaptation, or unsupported
   browser/fetch behavior.
4. Port the smallest coherent behavior and add a focused Rust test.
5. Run the relevant fixture or integration test before committing.
6. Keep one logical upstream behavior per commit so later resyncs remain easy
   to inspect and revert.

Useful verification commands are:

```bash
cargo test -p defuddle
cargo test -p defuddle --test fixture_harness -- --nocapture
cargo fmt --all -- --check
git diff --check
```

The fixture harness compares `tests/upstream/defuddle/fixtures/` with
`tests/upstream/defuddle/expected/` and validates
`tests/upstream-exceptions.toml`. A mismatch must be classified as one of:

- a Rust regression;
- an intentional Rust API or scope difference;
- an upstream fixture/output update that should be vendored; or
- a fixture that belongs in the exception manifest.

## Friction observed during the 6c39f09 to 6de1634 resync

The resync was not difficult because the overall pipeline had drifted. The
main-content, standardization, extractor, and Markdown phases were recognizable
in both repositories. The significant work came from boundaries where upstream
assumes browser-DOM semantics or broader runtime capabilities.

| Area | What made it effortful | Rust adaptation |
| --- | --- | --- |
| Extractor-output sanitization | Rust-generated extractor DOM needed an explicit output boundary and an allowlist for generated classes and attributes. | Centralized the sequence in `lib.rs::site_result()` and `standardize::standardize_extractor_output()`. |
| Selector parity | Several upstream selectors use `:has()` or compound negation that the Rust parser intentionally does not implement. | Added exact selector vocabulary where possible and manual, phase-local checks for unsupported compound selectors. |
| Related-content pruning | Node IDs can become invalid after ancestor removal, and image rows must not be mistaken for trailing cards in the middle of an article. | Added explicit subtree/sibling traversal and focused tests in `pipeline.rs`. |
| Footnotes | Definitions can live outside the selected content root, while upstream relies on flexible browser selectors and global DOM mutation. | Scanned the source document where needed, mapped references to canonical IDs, and kept canonical output in `footnotes/mod.rs`. |
| X conversations | Rendered X markup varies considerably. Rust selector results also need explicit document-order handling when multiple selector groups are combined. | Added timeline-cell ordering, fallback tweet discovery, resilient handle extraction, and self-thread classification in `extractors/twitter.rs`. |
| Gmail | The feature was a new adapter, but Rust has no `BaseExtractor` or JavaScript locale date parser, and the core must remain static-DOM only. | Added `extractors/gmail.rs`, an English calendar-date parser, DOM-only quote/chrome cleanup, and registry/integration coverage. |
| Markdown blank quotes | The behavior was a tiny postprocessing rule, but a missing end-of-line anchor initially changed existing fixture output. | Kept the rule isolated in `markdown.rs` and verified all active fixtures after correcting the regex. |

The practical conclusion is that the codebase has not deviated badly in
conceptual structure. The transition cost is predictable if a port starts by
locating the phase boundary and checking DOM, selector, and runtime assumptions.

## Resync record template

For a future batch, record the decisions in the pull request or commit message
using this shape:

```text
Upstream commit: <hash> <subject>
Classification: direct port | DOM/API adaptation | unsupported
Rust owner: <file>::<symbol>
Tests: <focused test>, <fixture/integration test>
Scope note: <why behavior is excluded or adapted, if applicable>
```
