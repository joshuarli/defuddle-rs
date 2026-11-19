STUFF ?= stuff
CHROMIUM ?= chromium
CRATES := htmd defuddle defuddle-cli defuddle-wasm

test:
	cargo test --workspace

.PHONY: coverage test-wasm

test-wasm:
	@mkdir -p .work/tmp/wasm-browser
	CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
	  cargo test --locked --target wasm32-unknown-unknown --target-dir .work/tmp/wasm-target -p defuddle-wasm
	cargo build --locked --target wasm32-unknown-unknown --target-dir .work/tmp/wasm-target -p defuddle-wasm
	wasm-bindgen --target web --out-name defuddle --out-dir .work/tmp/wasm-browser \
	  .work/tmp/wasm-target/wasm32-unknown-unknown/debug/defuddle_wasm.wasm
	cp crates/defuddle-wasm/tests/browser.html .work/tmp/wasm-browser/index.html
	python3 crates/defuddle-wasm/tests/run_browser.py .work/tmp/wasm-browser "$(CHROMIUM)"

coverage:
	@mkdir -p .work/tmp
	TMPDIR="$(CURDIR)/.work/tmp" cargo llvm-cov --locked --workspace --branch --summary-only

bench:
	cargo bench -p defuddle --bench fixture_bench

lint:
	cargo clippy --workspace --all-targets -- --deny warnings

publish:
	$(STUFF) publish cargo $(foreach c,$(CRATES),-p $(c))

publish-dry-run:
	$(STUFF) publish cargo $(foreach c,$(CRATES),-p $(c)) --dry-run
