Prefix: DEFR
Next: 10

### DEFR-2 · P3 · Fixture rewrites run on every extraction

- **Area:** Architecture
- **Where:** `crates/defuddle/src/lib.rs:537`, `crates/defuddle/src/overrides.rs:52`, `crates/defuddle/src/lib.rs:661`, `crates/defuddle/src/extractors/twitter.rs:666`
- **Problem:** Generic extraction calls `lookup_page_overrides` with the metadata title and then skips scoring, deletes nodes, or rewrites HTML for three titles (`Building Frontend UIs with Codex and Figma`, titles starting with `Node.js and CPU profiling`, and `Numeric Anchor-ID Footnote Pattern`). Any other page with one of those titles takes the same path. Every Markdown result also rewrites the literal `Vox_(website)` escape, and Twitter article links whose href is `//claude.md` are rewritten to `https://claude.md/`. These are fixture compensations on the production path, not test-only code.
- **Recommendation:** Take the hooks off the default parse path. Prefer deleting a hook whose fixture can be reclassified, and otherwise match the fixture's own URL rather than a title. Do not add the next fixture exception as another title branch or global string replace.
- **Fix with:** `dead-code-archaeology`
- **Effort:** M
- **Design:** [docs/rfc/DEFR-2-fixture-patches.md](docs/rfc/DEFR-2-fixture-patches.md)

### DEFR-3 · P3 · Shipped adapters have no parity check

- **Area:** Tests
- **Where:** `crates/defuddle/src/extractors/substack.rs:146`, `crates/defuddle/src/extractors/mastodon.rs:12`, `tests/upstream-exceptions.toml:12`, `tests/upstream-exceptions.toml:61`, `crates/defuddle/tests/fixture_harness.rs:138`
- **Problem:** `SubstackMode::Note` and `MastodonAdapter` extract real pages, but the harness skips `general--substack-note`, `general--substack-note-permalink`, and `comments--mastodon.social-@user-12345678` because the manifest still says those extractors are out of scope. No integration test asserts note or Mastodon output. A regression in either adapter is not checked.
- **Recommendation:** Run those three fixtures, classify each mismatch, and either drop the stale exception or record the actual output difference. Add one assertion for note mode and one for a Mastodon status if the fixtures stay excluded.
- **Fix with:** `test-audit`
- **Effort:** M

### DEFR-4 · P3 · Public crate exports the pipeline

- **Area:** Architecture
- **Where:** `crates/defuddle/src/lib.rs:53`, `crates/defuddle/src/lib.rs:71`, `crates/defuddle/src/dom.rs:107`
- **Problem:** The documented entry points are `parse_html` and `parse_html_profiled`. The crate also publishes `content_utils`, `extractors`, `markdown`, `sanitize`, `Metadata`, and `Document`. `NodeId` exposes its index. `defuddle-cli` and `defuddle-wasm` only need `parse_html`, `DefuddleOptions`, `adapter_names`, and `content_utils::is_absolute_url`. Every other public helper is a compatibility commitment at `0.0.0`, before a release.
- **Recommendation:** Keep the parse types and the two helpers the other crates call. Make `Document`, the extractor modules, and the markdown and sanitize helpers private, or move the URL check next to the CLI. Do this before the first publish.
- **Fix with:** `api-surface-and-module-boundaries`
- **Effort:** M
- **Design:** [docs/rfc/DEFR-4-public-surface.md](docs/rfc/DEFR-4-public-surface.md)

### DEFR-5 · P3 · Debug removal list is always empty

- **Area:** Operability
- **Where:** `crates/defuddle/src/lib.rs:675`, `crates/defuddle/src/pipeline.rs:684`, `crates/defuddle-cli/src/main.rs:361`
- **Problem:** `--debug` is documented as extraction diagnostics and is copied into JSON. The only removal record is `DebugInfo.removals`, which is always `Vec::new()`. `extract_content` takes `_debug` and does not use it. An operator cannot see which step dropped a node.
- **Recommendation:** Record removals from the steps that already delete nodes, or stop publishing `removals` and change the help text to the diagnostics that actually exist (`content_selector`, and `DEFUDDLE_PROFILE` phase timings).
- **Fix with:** `isolate-effects`
- **Effort:** M

### DEFR-6 · P3 · SingleFile inflate has no size cap

- **Area:** Security
- **Where:** `crates/defuddle-cli/src/main.rs:316`
- **Problem:** A file that contains the bytes `data-sfz` and `sfz-extra-data` is inflated with `read_to_end` and no uncompressed limit. A hostile archive can exhaust memory in any process that passes it to the CLI. The library path does not unzip.
- **Recommendation:** Cap the uncompressed entry, using the archive's uncompressed size when `rawzip` exposes it, and reject entries over that cap before reading.
- **Fix with:** `os-boundaries`
- **Effort:** S

### DEFR-7 · P3 · Twitter article links are interpolated into HTML

- **Area:** Security
- **Where:** `crates/defuddle/src/extractors/twitter.rs:671`, `crates/defuddle/src/lib.rs:372`
- **Problem:** `collect_formatted_text` writes `href` into the markup with `format!` and no `escape_html`. A quote in the href closes the attribute. `site_result` parses that string and strips scripts, event handlers, and dangerous URLs, so this is not an unfiltered script sink. Elements the sanitizer keeps, including an `https` iframe while `remove_iframes` is left at its default, still survive. The same function special-cases `//claude.md` (see DEFR-2). Other adapters escape text or use the DOM builder.
- **Recommendation:** Build the anchor with the DOM builder, or escape the href. Drop the `//claude.md` rewrite in the same change.
- **Fix with:** `os-boundaries`
- **Effort:** S

### DEFR-8 · P3 · Adapter names are a second list

- **Area:** Config
- **Where:** `crates/defuddle/src/extractors/mod.rs:51`, `crates/defuddle/src/extractors/mod.rs:86`
- **Problem:** `find_site_adapter` and `adapter_names` each list the eight adapters. URL gates are a third copy in `find_site_adapter` and again in `docs/site-adapters.md`. Adding a factory without updating `adapter_names` hides it from `--extractor` help consumers and from `list_extractors`. Nothing fails when the slices diverge.
- **Recommendation:** Drive names from the candidate slice. Keep the doc table, and test that `adapter_names()` equals the registry order.
- **Fix with:** `single-source-of-truth`
- **Effort:** S

### DEFR-9 · P4 · `make lint` rewrites the tree

- **Area:** Build
- **Where:** `Makefile:7`, `AGENTS.md:9`
- **Problem:** `make lint` runs `cargo clippy --fix --allow-dirty`. AGENTS.md documents a non-mutating clippy check. The make target applies fixes on a dirty tree, so a check command edits unrelated files.
- **Recommendation:** Make `lint` the AGENTS.md clippy command. Keep `--fix` off the default target.
- **Fix with:** `none`
- **Effort:** S
