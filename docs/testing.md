# Testing and verification

## Commands

Run the workspace build, test, and clippy commands in [AGENTS.md](../AGENTS.md). `--workspace` includes the opt-in `defuddle-wasm` adapter. The default members are `defuddle` and `defuddle-cli`. For a focused core check, use `cargo test -p defuddle`, `cargo test -p defuddle --test fixture_harness`, or `cargo test -p defuddle --test integration_tests`.

## WASM runtime

Use the pinned Rust toolchain with `wasm32-unknown-unknown`,
Node.js, and `wasm-bindgen-test-runner` 0.2.130 to run the
adapter tests inside WebAssembly.

```sh
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test --locked --target wasm32-unknown-unknown -p defuddle-wasm
```

To check the generated web bindings, use `wasm-bindgen` 0.2.130
and serve the smoke fixture on loopback.

```sh
mkdir -p .work/tmp/wasm-browser
cargo build --locked --target wasm32-unknown-unknown -p defuddle-wasm
wasm-bindgen --target web --out-name defuddle --out-dir .work/tmp/wasm-browser \
  "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/debug/defuddle_wasm.wasm"
cp crates/defuddle-wasm/tests/browser.html .work/tmp/wasm-browser/index.html
python3 -m http.server 0 --bind 127.0.0.1 --directory .work/tmp/wasm-browser
```

Open the printed listener URL in Chromium. The page displays
`PASS` after checking Unicode HTML and Markdown, domain,
iframe removal, and the extractor list.

## Upstream fixtures

The fixture harness compares HTML inputs in `tests/upstream/defuddle/fixtures/` with expected Markdown and HTML in `tests/upstream/defuddle/expected/`. It also validates `tests/upstream-exceptions.toml`. The snapshot source and pinned commit are recorded in [`tests/upstream/SNAPSHOT.md`](../tests/upstream/SNAPSHOT.md).

When output differs, determine whether the change is a Rust regression, an intentional API or scope difference, an upstream fixture update, or a fixture that belongs in the exception manifest. Do not change expected output without classifying the mismatch.

To inspect fixture-harness output around a named extractor:

```sh
cargo test -p defuddle --test fixture_harness -- --nocapture 2>&1 | grep -A 20 'EXTRACTOR comments--'
```

The fixture harness, integration tests, and metadata tests are under `crates/defuddle/tests/`; CLI tests are under `crates/defuddle-cli/tests/`.
