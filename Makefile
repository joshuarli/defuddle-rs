test:
	cargo test --workspace

bench:
	cargo bench -p defuddle --bench bench

bench-zim:
	DEFUDDLE_ZIM=$${DEFUDDLE_ZIM:-wikipedia_en_100_nopic_2026-07.zim} cargo bench -p defuddle --bench zim_bench

lint:
	cargo fmt --all
	cargo clippy --fix --allow-dirty --workspace --all-targets -- --deny warnings
