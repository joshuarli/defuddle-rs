STUFF ?= stuff
CRATES := htmd defuddle defuddle-cli defuddle-wasm

test:
	cargo test --workspace

bench:
	cargo bench -p defuddle --bench fixture_bench

lint:
	cargo clippy --workspace --all-targets -- --deny warnings

publish:
	$(STUFF) publish cargo $(foreach c,$(CRATES),-p $(c))
