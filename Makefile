STUFF ?= stuff
CRATES := htmd defuddle defuddle-cli defuddle-wasm

test:
	cargo test --workspace

.PHONY: coverage

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
