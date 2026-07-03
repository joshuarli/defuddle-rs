# Dependency Decisions

## HTML Parsing And DOM

The core crate uses `html5ever` as the parser foundation. It wraps the parsed tree in a Defuddle-owned `Document` adapter. Raw `RcDom` handles are not exposed in the public API.

This keeps the implementation aligned with the port plan:
- Defuddle needs whole-document traversal, scoring, retries, subtree mutation, and selected-node serialization.
- `lol_html` is not the primary implementation because it is a streaming rewriter and does not provide the full document tree needed by the extraction pipeline.
- `html5gum` is not the primary implementation because it is a tokenizer; the port needs tree construction, mutation, selector matching, and serialization.
- Raw `markup5ever_rcdom` is treated as a parser bridge, not as the production DOM interface.
- `scraper` is not currently used because Tranche 0 needs mutation and serialization more than read-only selection ergonomics.

The adapter spike currently proves the operations the extraction pipeline will need: parse, select, ancestor traversal, remove, unwrap, replace, append, clone/reparse, attribute mutation, text extraction, and inner/outer HTML serialization.

## Markdown Conversion

The Markdown crate is `html-to-markdown-rs` v3.8.1.

Reasoning:
- It is MIT licensed.
- It is compatible with the current Rust toolchain.
- It exposes extensive configuration options (heading style, link style, code block style, etc.) needed for Defuddle's custom output.
- It supports CSS selector-based element exclusion via `exclude_selectors`.

Custom Defuddle rules (footnote backrefs, callout blocks, math preservation, Table-based layout flattening) are layered on top of the standard conversion via HTML preprocessing in `src/standardize.rs`.

## Release Audit (Tranche 10)

The core crate (`defuddle`) has no network, TLS, proxy, async runtime, browser automation, or JavaScript engine dependencies. Verified via `cargo tree -p defuddle`.

The CLI crate (`defuddle-cli`) adds only `lexopt` (arg parsing) and `atty` (TTY detection), neither of which introduces network dependencies.


