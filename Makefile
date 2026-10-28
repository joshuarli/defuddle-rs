test:
	cargo test --workspace

bench:
	cargo bench -p defuddle --bench fixture_bench

lint:
	cargo clippy --workspace --all-targets -- --deny warnings
