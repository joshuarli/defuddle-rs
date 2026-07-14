test-strict:
	DEFUDDLE_FIXTURE_FILTER=table-layout--single-column DEFUDDLE_STRICT_FIXTURES=1 cargo test --test fixtures -- --nocapture > /tmp/fix-out.txt 2>&1; exit 0

test:
	cargo test --test fixtures

test-strict-full:
	DEFUDDLE_STRICT_FIXTURES=1 cargo test --test fixtures -- --nocapture
	cargo test --lib
	cargo test --test integration
	cargo test --test cli

lint:
	cargo fmt --all
	cargo clippy --fix --allow-dirty --workspace --all-targets -- --deny warnings
