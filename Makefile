test:
	cargo test --workspace

bench:
	cargo bench -p defuddle --bench bench

lint:
	cargo fmt --all
	cargo clippy --fix --allow-dirty --workspace --all-targets -- --deny warnings
