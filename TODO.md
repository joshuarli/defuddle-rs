# Remaining Work

## Fixture Parity (0/202 matching)

The golden fixture runner compares all 202 upstream fixtures against expected output but reports 0 byte-for-byte matches. The pipeline produces valid HTML/Markdown but differs from the TypeScript reference in formatting details.

### Required for fixture parity:
- Complete port of `src/markdown.ts` custom Turndown rules:
  - Exclamation-mark spacing before images
  - Ragged table column padding
  - Math/MathML-to-LaTeX conversion
  - Tag-like text escaping (e.g. `<foo>` appearing in code)
  - Parenthesis escaping in link destinations
  - Footnote backref links
  - Callout block rendering
- Complete port of `src/standardize.ts`:
  - Heading normalization (h1→h2, duplicate removal)
  - Code block detection and wrapping
  - Image deduplication (same alt, multi-resolution)
  - Callout/footnote standardization
  - Permalink anchor removal
  - Attribute stripping to allowed set
  - Wrapper element flattening
  - `<br>` normalization and consecutive collapse
  - Empty line removal
- Relative URL resolution and srcset parsing

## Missing Pipeline Features

- **Retry logic**: Upstream retries with `removePartialSelectors: false` when word count < 200, `removeHiddenElements: false` when < 50, and `removeLowScoring: false` / `removeContentPatterns: false` when still < 50. Not ported.
- **Schema text fallback**: When schema.org text is 1.5x longer than extracted content, re-extract from the schema-matched element on a sanitized clone. Not ported.
- **Content pattern removal**: Boilerplate, read time, article cards, eyebrow labels. Not ported.
- **Metadata block removal**: h1-adjacent date/author blocks removed from content. Not ported.
- **ScoreAndRemove**: Heuristic link-density-based block removal (`isLikelyContent`, `scoreNonContentBlock`). Functions exist in `scoring.rs` but are not wired into the pipeline.
- **Small image removal**: Requires computed pixel dimensions (browser only). Skipped in static-HTML mode.

## Browser-Only Features (Out of Scope)

- getComputedStyle / styleSheets media-rule evaluation
- Shadow DOM traversal and flattening
- React streaming SSR suspense resolution (`$RC("B:0","S:0")`)
- Mobile style application
- Live document mutation guards

## Site Extractors Not Ported

From `extractor-registry.ts`, the following are missing:
- Twitter/X (requires complex DOM paths + async Oembed)
- X Article surrogates (emoji surrogate pair repair)
- X Oembed (facet index adjustment for FxTwitter)
- YouTube (requires async transcript API)
- Claude, Gemini, Grok, Medium, Wikipedia, LinkedIn, Threads, Bluesky, Discourse, LeetCode, LWN, C2 Wiki, NYTimes

Bilibili is explicitly excluded per PLAN.md.

## Async Support

No async API. The Post-Core Tranche (separate `defuddle-extractors` crate) is not started. Required for YouTube transcripts, Reddit async fallback, and any API-backed extractors.

## Tooling

- No benchmarks (`cargo bench` and perf comparisons against TypeScript)
- `cargo fmt` and `cargo clippy` not run systematically
- Several dead_code warnings remain in unused constants/functions (scoring, pipeline, constants)
