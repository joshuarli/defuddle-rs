test:
	cargo test --test fixtures 2>&1 | tee /tmp/fixture-test-output.txt

test-strict:
	DEFUDDLE_STRICT_FIXTURES=1 cargo test --test fixtures -- --nocapture 2>&1 | tee /tmp/fixture-test-strict-output.txt

lint:
	cargo fmt --all -- --check
	cargo clippy --fix --allow-dirty --workspace --all-targets -- --deny warnings
