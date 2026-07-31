# Stage 4 validation

Stage 1 and Stage 3 runs use the same release benchmark command, fixture set, 10 warmups, and 100 timed iterations. The historical run uses the staged Stage 1 harness applied to `HEAD` before the Stage 2/3 source changes. The post-migration run uses the current worktree.

| Case | Before HTML | After HTML | Change | Before Markdown | After Markdown | Change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Hacker News listing | 364 µs | 297 µs | -18.4% | 2.39 ms | 1.93 ms | -19.2% |
| Hacker News post | 137 µs | 140 µs | +2.2% | 704 µs | 576 µs | -18.2% |
| Hacker News nested comments | 284 µs | 295 µs | +3.9% | 2.07 ms | 1.95 ms | -5.8% |
| Ordinary article (control) | 40.98 ms | 37.55 ms | -8.4% | 51.05 ms | 51.20 ms | +0.3% |
| Extractor-free document (control) | 1.84 ms | 1.79 ms | -2.7% | 3.78 ms | 4.04 ms | +6.9% |

These are single-run medians on the same machine, so they establish scale and regressions rather than a statistically stable performance claim. The controls are effectively unchanged. Hacker News HTML and Markdown sizes remain parity-compatible; the small latency differences are mixed and within the noise expected from this benchmark shape. macOS `sample` reported 6,352 KiB before and 6,032 KiB after for peak physical footprint, with no memory regression visible.

## Profile evidence

macOS `/usr/bin/sample` was used for five seconds against the release benchmark binary. The before snapshot showed the pre-adapter Hacker News extraction path alongside `markdown::html_to_markdown`. The after snapshot showed `HackerNewsAdapter::extract` and `Document::clone_subtree_from`; the adapter path has no `Document::append_html` or generated-HTML parse stack. The after snapshot did show `Document::append_html` in generic control/footnote work, which is expected and is not evidence of a Hacker News regression.

The test `extractors::hackernews::tests::adapter_keeps_dom_until_output_boundary` provides the stronger invariant: adapter extraction performs zero parses and serialization occurs exactly once when the content result is converted to final HTML. The raw profile files are intentionally not checked in; reproduce them with:

```bash
cargo bench -p defuddle --bench baseline --no-run
sample target/release/deps/baseline-* 5 -file /tmp/defuddle-stage4-profile.txt
```

## Boundary contract

- `ExtractionContext` owns the working input `Document`, original HTML for retry behavior, page URL, metadata, options, and diagnostics. It is internal and does not alter `DefuddleResult`.
- `SiteAdapter::matches` selects an adapter; `prepare` owns site-specific DOM preparation; `extract` returns `SiteExtraction` containing `ExtractedContent` and metadata variables.
- `ExtractedContent` owns a `Document` and root `NodeId`. Site extraction constructs or clones DOM nodes and never serializes them to continue processing.
- HTML serialization happens in `site_result` at the public result boundary. Markdown conversion consumes that final HTML representation, preserving the existing public API.
- All current site-specific implementations use the `SiteAdapter` boundary. The registry and adapter selector keep site extraction isolated from the generic pipeline.
