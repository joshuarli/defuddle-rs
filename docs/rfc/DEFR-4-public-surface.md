# DEFR-4: Public surface of the defuddle crate

Status: draft

## Summary

`defuddle` publishes its DOM, extractor modules, Markdown helpers, and sanitizer in addition to `parse_html`. The version is `0.0.0` and the crate is not on crates.io. Narrow the surface before the first release.

## Motivation

`Document` and `NodeId(pub usize)` let a caller build node ids the tree never issued. `content_utils`, `markdown`, and `sanitize` are pipeline helpers. The CLI uses `content_utils::is_absolute_url`. The WASM crate uses `extractors::adapter_names`. Nothing else outside the crate needs the rest. A published helper cannot move without a breaking change.

## Design

Keep public:

- `parse_html`, `parse_html_profiled`, and the option, result, and error types those functions use
- `extractors::adapter_names`
- one URL-shape helper if the CLI still needs it

Make `Document`, `NodeId`, `Metadata`, and the other functions in `content_utils`, `markdown`, `sanitize`, and `extractors` private. `Metadata` can stay public only if a caller needs the fields without going through `DefuddleResult`. Today `DefuddleResult` already carries them.

## Alternatives considered

Document the wide surface and treat it as the extension API. That freezes `Document`'s mutation methods and the Markdown preprocessor before they are stable.

Split a `defuddle-internal` crate. The workspace does not have a second consumer that needs that split.

## Open questions

- Should `adapter_names` stay on `extractors`, or move to the crate root next to `parse_html`? Recommendation: crate root, so the extractor module can be private.
- Should `Metadata` stay public? Recommendation: no. Return it only inside `DefuddleResult`.

## Decision

Pending.
