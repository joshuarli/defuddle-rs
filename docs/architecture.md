# Architecture

`defuddle::parse_html()` in [`crates/defuddle/src/lib.rs`](../crates/defuddle/src/lib.rs) accepts an HTML string and `DefuddleOptions`, then returns a `DefuddleResult`. Extraction is synchronous and uses the supplied document only.

## Parse flow

1. Parse HTML into the mutable `Document` tree and flatten declarative shadow-root templates.
2. Run document-wide pruning, then extract schema data, meta tags, and metadata.
3. If a site adapter matches, extract its content and pass it through sanitization and extractor-output standardization. This path bypasses generic content selection.
4. Otherwise, select main content or the requested content selector, fall back to the body when needed, remove unwanted content, apply page overrides, resolve URLs, standardize the selected subtree, sanitize it, and serialize HTML.
5. Convert the result to Markdown when requested.

The generic pipeline retries short results with less aggressive pruning, reparsing the original HTML for each retry. See [`lib.rs`](../crates/defuddle/src/lib.rs) and [`pipeline.rs`](../crates/defuddle/src/pipeline.rs) for the implementation.

## Module ownership

| Source | Responsibility |
| --- | --- |
| [`lib.rs`](../crates/defuddle/src/lib.rs) | Public options and results, metadata setup, adapter dispatch, retries, and result assembly. |
| [`dom.rs`](../crates/defuddle/src/dom.rs) | Mutable HTML document, node queries and mutations, and serialization. |
| [`pipeline.rs`](../crates/defuddle/src/pipeline.rs), [`prune.rs`](../crates/defuddle/src/prune.rs), [`scoring.rs`](../crates/defuddle/src/scoring.rs) | Generic content selection, pruning, scoring, and removal. |
| [`metadata.rs`](../crates/defuddle/src/metadata.rs), [`schema.rs`](../crates/defuddle/src/schema.rs) | Metadata, meta tags, and JSON-LD extraction. |
| [Site adapters](../crates/defuddle/src/extractors/mod.rs) | Site adapters and shared generated-output helpers. |
| [Standardization](../crates/defuddle/src/standardize/mod.rs) | Post-selection normalization, including callouts, elements, headings, images, tables, URLs, and whitespace. |
| [Footnotes](../crates/defuddle/src/footnotes/mod.rs), [embeds](../crates/defuddle/src/embeds/mod.rs) | Shared footnote canonicalization and static embed normalization. |
| [`sanitize.rs`](../crates/defuddle/src/sanitize.rs) | Unsafe elements, attributes, and URL schemes. |
| [`markdown.rs`](../crates/defuddle/src/markdown.rs) | HTML-to-Markdown conversion and Defuddle-specific normalization. |
| [`analysis.rs`](../crates/defuddle/src/analysis.rs) | Structural snapshot diagnostics collected only when profiling is enabled. |
| [`overrides.rs`](../crates/defuddle/src/overrides.rs), [`shadow_dom.rs`](../crates/defuddle/src/shadow_dom.rs) | Page-specific extraction exceptions and declarative shadow-root handling. |
| [`css_selector.rs`](../crates/defuddle/src/css_selector.rs), [`content_utils.rs`](../crates/defuddle/src/content_utils.rs), [`extraction_selectors.rs`](../crates/defuddle/src/extraction_selectors.rs) | Selector parsing, HTML/text helpers, and shared extraction selectors. |

## Boundaries

- Keep site-specific rules in the corresponding adapter. Generic cleanup belongs in the pipeline or standardization modules.
- Keep callouts, footnotes, and embeds as explicit shared-format boundaries.
- Sanitize both generic and adapter output regardless of extraction options. `sanitize_html_fragment()` also sanitizes HTML introduced from outside the parsed page DOM.
- Build generated markup with the extractor DOM builder or escape page-derived text with `content_utils::escape_html()`.
- `css_selector.rs` supports simple and descendant selectors, attribute tests, `:not()`, and simple descendant `:has()` checks. It does not implement `>`, `+`, or `~` combinators or every compound negation/`:has()` form; the generic pipeline handles a few cleanup cases manually.
- General Markdown conversion uses `htmd`; preprocessing and postprocessing are for Defuddle's canonical structures and compatibility rules.

## Site adapters

Site adapters are a separate synchronous, DOM-only boundary. See the [site adapter guide](site-adapters.md) for the registry, match gates, supported shapes, and extension steps.

## Extending shared normalization

Put a shared normalization rule in the narrowest existing standardization module, call it from `standardize_content()` in the established order, and add focused behavior coverage. For a new website, follow the [site adapter guide](site-adapters.md).
