# Repository guide

Defuddle is a synchronous Rust library and CLI for extracting readable content from static HTML.

## Build, test, check

- Build: `cargo build --workspace`.
- Test: `cargo test --workspace`.
- Check: `cargo clippy --workspace --all-targets -- --deny warnings`.
- Performance baselines use `rustybench` 0.4.0; see [performance](docs/performance.md).

## Invariants

- Core extraction uses only supplied HTML; it does not fetch URLs, execute JavaScript, or run asynchronously.
- Sanitize generic and site-adapter output regardless of extraction options.
- Keep site-specific behavior in its adapter; keep callouts, footnotes, and embeds as explicit shared boundaries.
- Keep generated extractor markup escaped or built through the DOM builder.
- Classify fixture mismatches before changing expected output or the exception manifest.

## Topics

- Public usage and outputs: [README](README.md).
- Pipeline, DOM, selectors, and module ownership: [architecture](docs/architecture.md).
- Site adapter matching, scope, and additions: [site adapters](docs/site-adapters.md).
- Build prerequisites, fixtures, and verification: [testing](docs/testing.md).
- Performance baseline workflow: [performance](docs/performance.md).
- Upstream resync workflow and Rust mapping: [upstream-resync skill](.agents/skills/upstream-resync/SKILL.md).
- Open tickets: [TODO](TODO.md). Draft decisions: [ignored options](docs/rfc/DEFR-1-ignored-options.md), [fixture patches](docs/rfc/DEFR-2-fixture-patches.md), [public surface](docs/rfc/DEFR-4-public-surface.md).
