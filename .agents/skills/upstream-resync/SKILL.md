---
name: upstream-resync
description: Port behavior changes from the TypeScript Defuddle repository into this Rust port. Use when reviewing or resynchronizing an upstream commit range.
---

# Upstream resync

Keep the Rust port behaviorally compatible where its synchronous, static-HTML contract allows. The TypeScript repository is the reference for supported extraction behavior; Rust owns its implementation and public API.

## Workflow

1. Read [resync context](references/resync-context.md) for the current source baseline, fixture snapshot, Rust ownership map, and adaptation boundaries.
2. Inspect the source range in the sibling `../defuddle` checkout:

   ```sh
   git -C ../defuddle log --oneline START..TARGET
   git -C ../defuddle diff START..TARGET -- src tests
   ```

   Use the last recorded Rust source resync commit as `START`; the vendored fixture snapshot is a separate baseline.
3. For each behavior change, inspect its upstream tests or fixtures, search for the exact behavior in Rust, and identify the owning phase before editing. Classify it as a direct port, a DOM/API adaptation, or unsupported browser/fetch behavior.
4. Port the smallest coherent behavior and cover observable behavior with a focused Rust test. Keep unrelated upstream changes out of the patch.
5. When fixture output differs, classify it as a Rust regression, an intentional Rust scope difference, an upstream fixture/output update to vendor, or a fixture exception. Do not refresh expected output before classifying the mismatch.
6. Run the narrowest relevant Rust test. For fixture changes or broad extraction behavior, run `cargo test -p defuddle --test fixture_harness -- --nocapture`. Run `git diff --check` before finishing.
7. Keep each logical upstream behavior independently reviewable. Record the new source resync baseline after completing the range.

The core crate receives supplied HTML only. Do not add URL fetching, JavaScript execution, or asynchronous extraction to it. A fetch-capable integration needs a separate crate and a caller-provided fetch boundary. Keep site-specific behavior in its adapter; shared callouts, footnotes, sanitization, and embeds belong at their established shared boundaries.

