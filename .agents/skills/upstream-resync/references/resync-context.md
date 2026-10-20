# Resync context

## Baselines

The Rust source resync covers upstream through `556d74e8c8fd68c49912d4b11a4ae9ec04d7b2b5`; the prior baseline was `49d14ecc4089696565c2479b2f57db72be199adc`. The companion YouTube extension was reviewed through the same upstream commit. Track these baselines separately when a resync covers only one owner.

The vendored fixture snapshot is independently pinned to `6c39f0959a2f566469a93a190b50e4b34bcf52c4`, recorded in `tests/upstream/SNAPSHOT.md`. Do not use it as the source resync baseline unless the Rust source baseline is intentionally reset.

The TypeScript source is [kepano/defuddle](https://github.com/kepano/defuddle). Use `../defuddle` when that checkout exists. Otherwise clone it to `.work/wt/upstream-defuddle`. The fixture harness compares `tests/upstream/defuddle/fixtures/` with `tests/upstream/defuddle/expected/` and checks `tests/upstream-exceptions.toml`.

## Upstream-to-Rust ownership

| Upstream area | Rust owner | Porting note |
| --- | --- | --- |
| Parse API, options, and result | `crates/defuddle/src/lib.rs` | Rust exposes a free function and owns parsing, metadata, adapter dispatch, retries, and result assembly. Browser, fetch, and async options do not automatically belong in the Rust API. |
| DOM operations | `crates/defuddle/src/dom.rs` | The mutable flat node store uses `NodeId` for mutations, subtree queries, and serialization. |
| Derived document facts | `crates/defuddle/src/analysis.rs` | Profiling collects a snapshot and empty mutation overlay; generic scoring still uses the mutable DOM. |
| Site adapter registry | `docs/site-adapters.md` | Rust checks a URL gate and then `SiteAdapter::matches()`. Candidate order defines priority. |
| Generic selection and removal | `crates/defuddle/src/pipeline.rs`, `prune.rs`, `scoring.rs` | Upstream behavior is spread across Rust modules. Search the behavior before choosing an owner. |
| Standardization | `crates/defuddle/src/standardize/mod.rs` | Preserve existing ordering and put rules in the narrowest current submodule. |
| Footnotes and callouts | `crates/defuddle/src/footnotes/mod.rs`, `standardize/callouts/mod.rs` | Footnote definitions can live outside selected content; callouts are a shared format boundary. |
| Images, headings, and elements | `crates/defuddle/src/standardize/images.rs`, `headings.rs`, `elements.rs` | Renderer-specific code-block heuristics are not part of the Rust port. |
| Markdown conversion | `crates/defuddle/src/markdown.rs` | General conversion uses `htmd`, with Defuddle-specific preprocessing and postprocessing. |
| Metadata and schema | `crates/defuddle/src/metadata.rs`, `schema.rs` | Metadata is extracted before content selection mutates the document. |
| URL and unsafe-element handling | `crates/defuddle/src/sanitize.rs`, `standardize/urls.rs` | Dangerous URL policy and relative URL resolution are separate operations. |
| Fetch-dependent YouTube transcripts | `../extensions/extension-youtube-transcript/src/youtube-api.ts`, `captions.ts`, `types.ts` | Keep network access and transcript selection in the extension. Follow `../extensions/AGENTS.md` and test in that repository. |

Confirm file paths and symbols against the current checkout before editing; ownership can move.

## Adaptation boundaries

Browser-DOM operations are methods on `Document` and use `NodeId`:

| Operation | Rust API |
| --- | --- |
| Query within a subtree | `doc.query_selector_all_in_subtree(element_id, selector)` |
| Query the document | `doc.query_selector_all(selector)` |
| Find the nearest ancestor | `doc.closest(element_id, selector)` |
| Read inner or outer HTML | `doc.inner_html(element_id)` / `doc.outer_html(element_id)` |
| Remove or replace a node | `doc.remove(element_id)` / `doc.replace_with_html(element_id, html)` |
| Clone nodes | Clone the `Document` or use the DOM builder for generated output. |

Check that a `NodeId` remains live after removing or replacing its ancestor. Sort collected IDs explicitly when document order matters: comma-selector results can be grouped by selector rather than returned in browser order. Check `docs/architecture.md` for selector limits.

Rust adapters return `SiteExtraction`; `lib.rs::site_result()` sanitizes unsafe elements, resolves URLs, standardizes extractor output, serializes it, and optionally converts it to Markdown. Do not run generic page scoring or boilerplate removal over already extracted site output. Generated extractor classes that must survive cleanup belong in `extractors::comments::is_extractor_class()`.

The upstream `BaseExtractor` carries `schemaOrgData`, `includeReplies`, `language`, and an optional `fetch` function. Rust adapters receive the shared `pipeline::ExtractionContext`, which provides the parsed DOM, page URL, metadata, schema data, and `DefuddleOptions`. Decide whether an upstream option changes the public Rust contract or is only a constructor detail before adding it. Do not add fetch, transcript retrieval, or async extraction to the core crate. The Rust registry excludes fetch-dependent paths such as YouTube and Bilibili transcripts and Reddit Atom feeds. The companion YouTube extension owns transcript fetching, caption selection, and its asynchronous tests.

## Known scope gaps

The Rust whitespace pass does not implement upstream's general `removeEmptyLines` behavior for spacing adjacent inline elements. The existing `standardize/whitespace.rs::move_inline_whitespace` handles a narrower set of cases. Refinements to the upstream pass are incomplete ports until the base behavior is added; treat that base behavior as a separate feature decision.

Upstream `content-boundary.ts` helpers and several `removeByContentPattern` rules do not have direct equivalents in the restructured Rust `pipeline::remove_content_patterns`. Search for behavior rather than porting upstream helpers by name; a missing equivalent may be intentional restructuring or an explicit-page-URL boundary.

Earlier resyncs found recurring adaptation work around selector limitations, explicit document order, stale `NodeId`s after ancestor removal, footnote definitions outside selected content, and sanitizing generated extractor output. Check the owning phase and its tests for existing handling before adding another path.

## Resync record

Record decisions in the pull request or commit message in this shape:

```text
Upstream commit: <hash> <subject>
Classification: direct port | DOM/API adaptation | unsupported
Rust owner: <file>::<symbol>
Tests: <focused test>, <fixture/integration test>
Scope note: <why behavior is excluded or adapted, if applicable>
```

