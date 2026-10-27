# Testing and verification

## Commands

Run the workspace build, test, and clippy commands in [AGENTS.md](../AGENTS.md). `--workspace` includes the opt-in `defuddle-wasm` adapter. The default members are `defuddle` and `defuddle-cli`. For a focused core check, use `cargo test -p defuddle`, `cargo test -p defuddle --test fixture_harness`, or `cargo test -p defuddle --test integration_tests`.

## Upstream fixtures

The fixture harness compares HTML inputs in `tests/upstream/defuddle/fixtures/` with expected Markdown and HTML in `tests/upstream/defuddle/expected/`. It also validates `tests/upstream-exceptions.toml`. The snapshot source and pinned commit are recorded in [`tests/upstream/SNAPSHOT.md`](../tests/upstream/SNAPSHOT.md).

When output differs, determine whether the change is a Rust regression, an intentional API or scope difference, an upstream fixture update, or a fixture that belongs in the exception manifest. Do not change expected output without classifying the mismatch.

To inspect fixture-harness output around a named extractor:

```sh
cargo test -p defuddle --test fixture_harness -- --nocapture 2>&1 | grep -A 20 'EXTRACTOR comments--'
```

The fixture harness, integration tests, and metadata tests are under `crates/defuddle/tests/`; CLI tests are under `crates/defuddle-cli/tests/`.
