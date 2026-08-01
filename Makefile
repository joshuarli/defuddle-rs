test:
	cargo test --workspace

bench:
	DEFUDDLE_HTML_DIR=$${DEFUDDLE_HTML_DIR:?set DEFUDDLE_HTML_DIR to an extracted HTML directory} cargo bench -p defuddle --bench zim_bench

extract-zim-html:
	DEFUDDLE_ZIM=$${DEFUDDLE_ZIM:-wikipedia_en_100_nopic_2026-07.zim} DEFUDDLE_HTML_DIR=$${DEFUDDLE_HTML_DIR:-zim-html} cargo bench -p defuddle --bench zim_extract

lint:
	cargo fmt --all
	cargo clippy --fix --allow-dirty --workspace --all-targets -- --deny warnings
