# DEFR-2: Fixture patches on the production path

Status: draft

## Summary

Three title-keyed overrides, one global Markdown replace, and one Twitter href rewrite compensate for specific fixtures during every extraction.

## Motivation

`lookup_page_overrides` runs from `parse_internal_once` on the metadata title. A page titled `Building Frontend UIs with Codex and Figma` skips ordinary removal and cuts the DOM for that Figma post. Titles starting with `Node.js and CPU profiling`, and the title `Numeric Anchor-ID Footnote Pattern`, have their own hooks. The match is the title string, not the fixture file.

`parse_internal_once` also replaces `Vox_(website)` in every Markdown result. Twitter article conversion rewrites `//claude.md` to `https://claude.md/`. The next fixture miss will be copied as another branch of the same kind.

## Design

Remove each hook from the default path.

1. Re-run the affected fixtures.
2. If the difference is an accepted Rust gap, record it in `tests/upstream-exceptions.toml` and delete the hook.
3. If the output must stay, gate the hook on that fixture's URL, not the title, and keep it out of unrelated documents.

Do not document the titles as supported special cases.

## Alternatives considered

Leave the hooks and list the titles in the architecture doc. A second article with the same title still loses content, and the doc would bless the pattern.

Gate them with a `test_mode` flag defaulting off. The fixture harness would have to remember to set it, and production parity for those pages would change either way.

## Open questions

- For the Figma, Node.js profiling, and numeric-footnote titles, is matching the current fixture output still required? Recommendation: no, unless a fixture fails without the hook and the difference is not an accepted gap. The Vox replace and the `//claude.md` rewrite should go even if a fixture changes.

## Decision

Pending.
