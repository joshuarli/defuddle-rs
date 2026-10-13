# Upstream synchronization

This repository is a behaviorally compatible Rust port of `../defuddle`, not a
line-for-line translation. The TypeScript repository is the reference for
supported extraction behavior; the Rust repository owns its synchronous,
static-HTML implementation and public API.

The vendored fixture snapshot in [`tests/upstream/SNAPSHOT.md`](../tests/upstream/SNAPSHOT.md)
is based on `6c39f0959a2f566469a93a190b50e4b34bcf52c4`. The last recorded
source resync covered upstream through `49d14ecc4089696565c2479b2f57db72be199adc`;
the prior resync covered through `d4c4bad0dc96ca31b2383328e38061cc1490db47`.
Review fixture changes separately from source changes; do not update golden
output just because upstream output changed.

## Upstream-to-Rust map

The [TypeScript source](https://github.com/kepano/defuddle) can be checked out
as a sibling at `../defuddle`. This map gives the Rust owner for each shared
behavior; the Rust links resolve inside this repository.

| Upstream area | Rust owner | Porting note |
| --- | --- | --- |
| Parse API, options, and result | [`core API`](../crates/defuddle/src/lib.rs) | Rust exposes a free function and owns parsing, metadata, adapter dispatch, retries, and result assembly. Browser, fetch, and async options do not automatically belong in the Rust API. |
| DOM operations | [`DOM adapter`](../crates/defuddle/src/dom.rs) | A mutable flat node store uses `NodeId` for mutations, subtree queries, and serialization. |
| Derived document facts | [`analysis snapshot`](../crates/defuddle/src/analysis.rs) | A snapshot and empty mutation overlay are collected only when profiling is enabled; generic scoring still uses the mutable DOM. |
| Site adapter registry | [site adapter guide](site-adapters.md) | Rust checks a URL gate and then `SiteAdapter::matches()`. Candidate order defines priority. |
| Generic selection and removal | [`content pipeline`](../crates/defuddle/src/pipeline.rs), [`pruning`](../crates/defuddle/src/prune.rs), [`scoring`](../crates/defuddle/src/scoring.rs) | Upstream removal behavior is spread across several Rust modules. Search the behavior before choosing an owner. |
| Standardization | [`standardization`](../crates/defuddle/src/standardize/mod.rs) | Preserve the existing ordering and put new rules in the narrowest current submodule. |
| Footnotes and callouts | [`footnote handling`](../crates/defuddle/src/footnotes/mod.rs), [`callout handling`](../crates/defuddle/src/standardize/callouts/mod.rs) | Footnote definitions can live outside selected content; callouts are a shared format boundary. |
| Images, headings, and elements | [`image normalization`](../crates/defuddle/src/standardize/images.rs), [`heading normalization`](../crates/defuddle/src/standardize/headings.rs), [`element normalization`](../crates/defuddle/src/standardize/elements.rs) | The Rust port does not include renderer-specific code-block heuristics. |
| Markdown conversion | [`Markdown conversion`](../crates/defuddle/src/markdown.rs) | General conversion uses `htmd`, surrounded by Defuddle-specific preprocessing and postprocessing. |
| Metadata and schema | [`metadata`](../crates/defuddle/src/metadata.rs), [`schema extraction`](../crates/defuddle/src/schema.rs) | Metadata is extracted before content selection mutates the document. |
| URL and unsafe-element handling | [`sanitization`](../crates/defuddle/src/sanitize.rs), [`URL resolution`](../crates/defuddle/src/standardize/urls.rs) | Dangerous URL policy and relative URL resolution remain separate operations. |

Adapter names, URL gates, supported page shapes, and module links are listed
in the [site adapter guide](site-adapters.md).

The Rust adapter registry is intentionally smaller. YouTube and Bilibili
transcripts, Reddit Atom feeds, and other fetch-dependent paths are outside the
core crate because its contract is synchronous, DOM-only, and network-free. Do
not add a network client or async runtime to `defuddle` to close that gap. A
future fetch-capable integration belongs in a separate crate with a
caller-provided fetch abstraction.

## DOM differences

Upstream code can use browser-DOM methods directly. In Rust, corresponding
operations are methods on `Document` and identify nodes with `NodeId`:

| Operation | Rust API |
| --- | --- |
| Query within a subtree | `doc.query_selector_all_in_subtree(element_id, selector)` |
| Query the document | `doc.query_selector_all(selector)` |
| Find the nearest ancestor | `doc.closest(element_id, selector)` |
| Read inner or outer HTML | `doc.inner_html(element_id)` / `doc.outer_html(element_id)` |
| Remove or replace a node | `doc.remove(element_id)` / `doc.replace_with_html(element_id, html)` |
| Clone nodes | Clone the `Document` or use the DOM builder for generated output. |

Do not retain a `NodeId` after removing or replacing its ancestor without
checking whether it is still live. When a rule needs document order, sort
collected IDs explicitly: comma-selector results can be grouped by selector
instead of returned in browser document order.

Do not retain a `NodeId` after removing or replacing its ancestor without
checking whether it is still live. Sort collected IDs when document order
matters; comma-selector results can be grouped by selector rather than returned
in browser order. See [selector limits](architecture.md#boundaries).

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

The upstream constructor also forwards `includeReplies` and an optional fetch
function. Rust has no corresponding cross-adapter options, and Reddit's Atom
feed fallback is outside the static, network-free core contract. Keep those
behaviors out of the core until the public API and fetch boundary are designed
for them.

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
git diff --check
```

The fixture harness compares `tests/upstream/defuddle/fixtures/` with
`tests/upstream/defuddle/expected/` and validates
`tests/upstream-exceptions.toml`. A mismatch must be classified as one of:

- a Rust regression;
- an intentional Rust API or scope difference;
- an upstream fixture/output update that should be vendored; or
- a fixture that belongs in the exception manifest.

The historical resync notes below preserve source-relative paths: Rust file
names are relative to the core crate source directory, while TypeScript source
names are relative to the sibling upstream checkout.

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

## Scope gaps found during the 6de1634 to d4c4bad resync

Four upstream commits in this range were ported: SVG SMIL sanitization,
schema.org text fallback sanitization, declarative shadow DOM flattening, and
`<template>` sanitization hardening (GHSA-3cr7-rgx8-76c3). The rest of the
range was CI/docs/website-only, out of scope by the existing site-extractor
and fetch boundaries (C2 wiki, YouTube caption language, CLI fetch-failure
exit), already excluded by `tests/upstream-exceptions.toml` (arXiv LaTeXML
equation-table-to-MathML flattening), a linkedom-runtime-only bug with no
Rust equivalent code path (uppercase heading `tagName` round-trip and its
fixture correction), or already correct by construction (the `htmd` crate
already grows code-fence length past embedded backticks; `metadata.rs`
already takes an explicit `page_url` instead of reading `doc.location`).

One gap is real but out of scope for a resync-sized change: upstream's
`removeEmptyLines` in `standardize.ts` inserts a space between adjacent
inline elements when text was likely joined by removing a wrapping span,
skipping footnote references, `sub`/`sup`/`code`, and text already ending or
starting with punctuation, a closing/opening quote, or a dash. The Rust port
never carried this general pass — `standardize/whitespace.rs::move_inline_whitespace`
covers only a narrower set of cases (leading/trailing whitespace inside
`code`/`em`/`strong`/`a`/`mark`/`s`/`del`, plus `code`-then-alphanumeric
spacing). Upstream commits dda0e9f and 78bdabc are refinements to the pass
Rust doesn't have; porting them without first building the base pass would
be incomplete. The one upstream fixture this touches
(`footnotes--maggieappleton.com-xanadu-patterns`) is already excepted for an
unrelated reason. Treat adding the general pass as its own deliberate
feature decision, not a resync patch.

Similarly, `content-boundary.ts` (`isAboveContentStart`, the `isNodeBefore`
document-order helper) and several rules inside `removeByContentPattern`
(`isLabeledMetadata`, the "article metadata header block" removal, the
breadcrumb-link document-order check) were not found in `pipeline.rs`'s
`remove_content_patterns` in their upstream form — that function was
restructured during porting rather than transliterated. Upstream commit
a212ac4 patches exactly these un-ported rules plus a `doc.location`
fallback that can't exist in this port's explicit-`page_url` design; none of
it has a current target.

## Resync from d4c4bad to 49d14ec

Upstream commit `49d14ec Update agents file` moved repository guidance from
`CLAUDE.md` to `AGENTS.md`. It changed no `src/`, `tests/`, fixtures, or expected
output, so the vendored fixture snapshot remains pinned to `6c39f09`.

The new security guidance made an existing sanitizer mismatch explicit:
upstream `Defuddle._stripUnsafeElements()` removes every attribute whose name
starts with `on`, while Rust `sanitize::strip_unsafe_elements()` had a fixed
handler list. The Rust DOM now removes all such attributes through
`dom::Document::remove_event_handler_attrs()` without copying attribute names.
The focused `sanitize::tests::strips_unlisted_event_handler_attributes` test
failed before the change and passes afterward; `strips_event_handlers` also
covers the public `parse_html()` result. `AGENTS.md` now records the
corresponding Rust output-boundary rule. The rest of the upstream commit is
guidance for its browser, Node, CLI, and Worker runtimes rather than a Rust
source change. The complete fixture harness passed. One five-sample
`rustybench` pass on the fixed 50-page corpus measured 12.04 seconds with
unchanged allocation counters, compared with the saved 12.43-second baseline;
one pass is only a regression check, not evidence of a repeatable speedup.

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
