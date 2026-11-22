STUFF ?= stuff
CHROMIUM ?= chromium
CRATES := htmd defuddle defuddle-cli defuddle-wasm

test:
	@mkdir -p .work/tmp
	python3 -m unittest discover -s tests -p bench_driver_test.py
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
	python3 scripts/bench.py run $(BENCH_ARGS)

bench-check:
	python3 scripts/bench.py check $(BENCH_ARGS)

bench-list:
	python3 scripts/bench.py list

bench-corpus:
	cargo bench -p defuddle --bench fixture_bench

lint:
	cargo clippy --workspace --all-targets -- --deny warnings

publish:
	$(STUFF) publish cargo $(foreach c,$(CRATES),-p $(c))

publish-dry-run:
	$(STUFF) publish cargo $(foreach c,$(CRATES),-p $(c)) --dry-run

.PHONY: clean
clean:
	@if test -L ./target || { test -e ./target && ! test -d ./target; }; then \
	  printf '%s\n' 'make clean: ./target must be a local build directory' >&2; exit 1; \
	fi
	rm -rf -- ./target
