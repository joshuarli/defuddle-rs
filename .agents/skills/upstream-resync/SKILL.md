---
name: upstream-resync
description: Port behavior changes from the TypeScript Defuddle repository into the Rust port and its companion extensions. Use when reviewing or resynchronizing an upstream commit range.
---

# Upstream resync

Keep the Rust port behaviorally compatible where its synchronous, static-HTML contract allows. The TypeScript repository is the reference; Rust owns its implementation and public API. Keep fetch-dependent behavior in the companion extensions repository when an extension owns it.

The upstream source repository is [kepano/defuddle](https://github.com/kepano/defuddle). Use an existing sibling `../defuddle` checkout when available. If it is absent, clone upstream under `.work/wt/upstream-defuddle`; keep source checkouts and scratch state inside `.work/` and do not create a sibling checkout.

## Workflow

1. Read [resync context](references/resync-context.md) for the current source baseline, fixture snapshot, Rust ownership map, and adaptation boundaries.
2. Set `UPSTREAM` to the available checkout path, then inspect the source range:

   ```sh
   UPSTREAM=.work/wt/upstream-defuddle
   git -C "$UPSTREAM" log --oneline START..TARGET
   git -C "$UPSTREAM" diff START..TARGET -- src tests
   ```

   Use the last recorded Rust source resync commit as `START`; the vendored fixture snapshot is a separate baseline.
3. For each behavior change, inspect its upstream tests or fixtures, search for the exact behavior in Rust and `../extensions`, and identify the owning phase before editing. Classify it as a direct port, a DOM/API adaptation, an extension port, or unsupported behavior.
4. Port the smallest coherent behavior and cover it with a focused test in the owning repository. Read that repository's `AGENTS.md` and follow its test commands.
5. When fixture output differs, classify it as a Rust regression, an intentional Rust scope difference, an upstream fixture/output update to vendor, or a fixture exception. Do not refresh expected output before classifying the mismatch.
6. Run the narrowest relevant Rust test. For fixture changes or broad extraction behavior, run `cargo test -p defuddle --test fixture_harness -- --nocapture`. Run `git diff --check` before finishing.
7. Keep each logical upstream behavior independently reviewable. Record the new source resync baseline after completing the range.

The core crate receives supplied HTML only. Do not add URL fetching, JavaScript execution, or asynchronous extraction to it. Fetch-dependent YouTube transcript behavior belongs in `../extensions/extension-youtube-transcript`, which owns the fetch boundary and tests. Keep site-specific behavior in its adapter; shared callouts, footnotes, sanitization, and embeds belong at their established shared boundaries.

