test:
	cargo test --workspace

bench:
	cargo bench -p defuddle --bench fixture_bench

lint:
	cargo clippy --fix --allow-dirty --workspace --all-targets -- --deny warnings
