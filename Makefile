test:
	cargo test --test fixtures

test-strict:
	DEFUDDLE_STRICT_FIXTURES=1 cargo test --test fixtures -- --nocapture

lint:
	cargo fmt --all -- --check
	cargo clippy --fix --allow-dirty --workspace --all-targets -- --deny warnings
