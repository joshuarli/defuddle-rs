lint:
	cargo fmt --all -- --check
	cargo clippy --fix --allow-dirty --workspace --all-targets -- --deny warnings
