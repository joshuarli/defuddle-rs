# Testing and verification

## Commands

Run the workspace build, test, and clippy commands in [AGENTS.md](../AGENTS.md). `--workspace` includes the opt-in `defuddle-wasm` adapter. The default members are `defuddle` and `defuddle-cli`. For a focused core check, use `cargo test -p defuddle`, `cargo test -p defuddle --test fixture_harness`, or `cargo test -p defuddle --test integration_tests`.

## WASM runtime

Use the pinned Rust toolchain with `wasm32-unknown-unknown`,
Node.js, Chromium, Python 3, and wasm-bindgen-cli 0.2.130.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
make test-wasm CHROMIUM=chromium
```

The test runs the adapter cases in Node, builds the web
bindings, then serves the browser fixture on loopback. The
headless browser checks Unicode HTML and Markdown, domain,
iframe removal, and the extractor list. CI uses the GitHub
Ubuntu runner's Chrome and logs its version and Node's version.

## Upstream fixtures

The fixture harness compares HTML inputs in `tests/upstream/defuddle/fixtures/` with expected Markdown and HTML in `tests/upstream/defuddle/expected/`. It also validates `tests/upstream-exceptions.toml`. The snapshot source and pinned commit are recorded in [`tests/upstream/SNAPSHOT.md`](../tests/upstream/SNAPSHOT.md).

When output differs, determine whether the change is a Rust regression, an intentional API or scope difference, an upstream fixture update, or a fixture that belongs in the exception manifest. Do not change expected output without classifying the mismatch.

To inspect fixture-harness output around a named extractor:

```sh
cargo test -p defuddle --test fixture_harness -- --nocapture 2>&1 | grep -A 20 'EXTRACTOR comments--'
```

The fixture harness, integration tests, and metadata tests are under `crates/defuddle/tests/`; CLI tests are under `crates/defuddle-cli/tests/`.
