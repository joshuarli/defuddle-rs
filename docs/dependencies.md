# Dependency Decisions

## HTML Parsing And DOM

The core crate uses `html5ever` as the parser foundation. It currently parses through `markup5ever_rcdom` and immediately wraps the parsed tree in a Defuddle-owned `Document` adapter. Raw `RcDom` handles are not exposed in the public API.

This keeps the implementation aligned with the port plan:
- Defuddle needs whole-document traversal, scoring, retries, subtree mutation, and selected-node serialization.
- `lol_html` is not the primary implementation because it is a streaming rewriter and does not provide the full document tree needed by the extraction pipeline.
- `html5gum` is not the primary implementation because it is a tokenizer; the port needs tree construction, mutation, selector matching, and serialization.
- Raw `markup5ever_rcdom` is treated as a parser bridge, not as the production DOM interface.
- `scraper` is not currently used because Tranche 0 needs mutation and serialization more than read-only selection ergonomics.

The adapter spike currently proves the operations the extraction pipeline will need: parse, select, ancestor traversal, remove, unwrap, replace, append, clone/reparse, attribute mutation, text extraction, and inner/outer HTML serialization.

## Markdown Candidate

The initial Markdown crate candidate is `html-to-markdown-rs`.

Reasoning:
- It is MIT licensed.
- Its declared Rust version is compatible with the current toolchain.
- It exposes feature flags that should allow avoiding heavy optional capabilities.
- It appears more suitable than `html2md`, which is GPL-licensed, and `mdream`, whose current declared Rust version is newer than this workspace toolchain.

This is not yet a committed implementation dependency. Before Tranche 5, a spike must prove that the selected crate can support Defuddle's custom table, footnote, math, callout, media, and escaping rules without invasive work. If it cannot, the plan must be revised before writing a bespoke renderer.

