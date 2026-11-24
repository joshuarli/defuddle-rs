# Repository guide

Defuddle is a synchronous Rust library and CLI for extracting readable content from static HTML.

The roadmap, [docs/roadmap.md](docs/roadmap.md), holds all current and future work; its Status is the resume point.

## Build, test, check

- Build: `cargo build --workspace`.
- Test: `cargo test --workspace`.
- Check: `cargo clippy --workspace --all-targets -- --deny warnings`.
- Performance baselines use `rustybench` 0.4.0; see [performance](docs/performance.md).

## Verification

`make clean` removes `./target` and refuses symlinked outputs.

- `make test` ran 161 workspace tests on macOS ARM64. It took 945.43 s with an empty target directory and 4.55 s on the warm repeat. Warm `make lint` took 0.36 s. These measurements include concurrent host load. Release performance requires a readiness-qualified run.
- Library consumers include the CLI and WASM adapter; core API, extraction, sanitization and Markdown are exercised by `crates/defuddle/tests/integration_tests.rs`, `metadata_tests.rs`, and `fixture_harness.rs`. The harness checks 168 upstream inputs, 168 expected Markdown files and 3 expected HTML files.
- CLI flags, input files, JSON, Markdown, frontmatter and success/failure status are exercised by `crates/defuddle-cli/tests/cli_tests.rs`. WASM JSON and extractor enumeration run in the native and wasm32 test suites. The generated web bindings also have a [Chromium smoke test](docs/testing.md#wasm-runtime). There are no network endpoints.
- Edit tier: `cargo test --locked -p defuddle --test integration_tests` for extraction, `cargo test --locked -p defuddle-cli --test cli_tests` for CLI, and `cargo test --locked -p defuddle-wasm` for the adapter; costs depend on compilation.
- Change tier: `make test` covers the workspace; on a contended host the warm baseline and candidate took 7.39 s and 5.43 s. `make lint` is the static check. Scenario tier: `make test-wasm` exercises Node and Chromium; cost is unmeasured here. Full: the fixture benchmark is manual and separate.
- Representative extraction output and sanitization are guarded by `integration_tests` and `fixture_harness`; metadata precedence by `metadata_tests`. CLI failure status and destination preservation are guarded by `cli_tests` and `cli_tty`. WASM JSON, extractor selection and enumeration are guarded by the adapter tests and Chromium fixture.
- For CLI-only changes, `cargo test --locked -p defuddle-cli --test cli_tests`
  checks the built binary's exit status, stdin, output formats, and destination files.
  On Unix, `cargo test --locked -p defuddle-cli --test cli_tty` checks the
  missing-input diagnostic and exit status with terminal stdin.
- `make coverage` uses cargo-llvm-cov 0.9.1 and the pinned
  nightly's LLVM tools to report workspace line and branch coverage.
- Use the public `parse_html()` fixture-corpus workload for extraction hot-path changes. The fixture executable uses the Rustybench version in `Cargo.lock`. [Performance](docs/performance.md)
- Vendored fixtures: [kepano/defuddle](https://github.com/kepano/defuddle) at `6c39f0959a2f566469a93a190b50e4b34bcf52c4` ([snapshot](tests/upstream/SNAPSHOT.md)); vendored [htmd](https://github.com/letmutex/htmd) 0.5.6 ([README](crates/htmd/README.md)).
- The default CI and lint runs passed, and the fleet's test-workflow rule found no violations. The test workflow selects nightly 2026-09-15. The local pin is nightly 2026-11-09.

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
