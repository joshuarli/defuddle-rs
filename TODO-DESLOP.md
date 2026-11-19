Prefix: DSLP
Next: 8

Deslop campaign backlog ([plan](https://github.com/laputa-systems/stuff/blob/main/docs/campaigns/deslop.md), tier B). One agent works this file with the `deslop` skill: take the lowest-numbered open ticket whose area does not start with `Needs owner ·`. After DSLP-5, work the open P1 and P2 findings, then fill in `## Report` below.

## Report

Ground (DSLP-1) measured default revision
`63a23ca89d409d2e211991d5bd8ebfc1dc349716` on macOS ARM64.
Both `make test` runs passed 161 cases. An empty target directory
took 945.43 s and the warm repeat took 4.55 s. Warm
`make lint` took 0.36 s. The host was busy, so these
are observed command costs. The reported release workload is
the existing public HTML and Markdown fixture benchmark.
`perf-gate-ready` rejected the host for release measurements.
The [verification guide](AGENTS.md#verification) records the
public boundaries, consumers, vendored revisions and CI pins.
Both ordinary default checks and the fleet test-workflow rule passed.
[Default tests](https://github.com/joshuarli/defuddle-rs/actions/runs/43925764730) ·
[Default lint](https://github.com/joshuarli/defuddle-rs/actions/runs/43925764944)

## Tickets



### DSLP-3 · P2 · Types: strict profile and typing findings

- **Area:** deslop · types
- **Where:** type checker and lint configuration; Rust sources
- **Problem:** The strict-profile candidates in the deslop skill's tools reference have not been measured here.
- **Recommendation:** Run the deslop Types step. Accept: clean at the adopted profile, escape hatches not increased, witnesses recorded.
- **Fix with:** deslop
- **Effort:** M

### DSLP-4 · P2 · Tests: coverage, boundary gaps, mutation

- **Area:** deslop · tests
- **Where:** test suites and CI
- **Problem:** Coverage and mutation results are unmeasured; no deslop mutation lane exists.
- **Recommendation:** Run the deslop Tests step. Accept: no undispositioned survivor in critical code, the test cost budget holds, and the lane is green where the tools are mirrored.
- **Fix with:** deslop
- **Effort:** M

### DSLP-5 · P2 · Dead code: tools and debloat

- **Area:** deslop · dead code
- **Where:** whole repository
- **Problem:** No dead-code scan has run.
- **Recommendation:** Run the deslop Dead code step. Accept: tools clean or allowlisted with reasons; walk-backs filed as Needs owner tickets.
- **Fix with:** deslop
- **Effort:** M


### DSLP-6 · P2 · Retry skips document-wide pruning

- **Area:** extraction · retries
- **Where:** crates/defuddle/src/lib.rs:259, crates/defuddle/src/lib.rs:465, crates/defuddle/src/extractors/substack.rs:412
- **Problem:** Initial parsing runs the prune registry, but both short-result retries reparse the original HTML without it and can win on word count. Reproduce: printf %s '<html><body><article><p>Short article text.</p><div><hr></div><p>For More news and commentary visit our publication every day.</p><p>Subscribe to receive every new article and more special reports.</p></article></body></html>' | cargo run -q -p defuddle-cli -- --json; output includes both footer paragraphs even though the initial pruning rule removes them.
- **Recommendation:** Apply the same document preparation to each retry before extraction, retaining the intentional retry option changes; cover the resulting public output with a short-page regression witness.
- **Fix with:** root-cause
- **Effort:** S


### DSLP-7 · P2 · Invalid property can truncate the output file

- **Area:** CLI · output
- **Where:** crates/defuddle-cli/src/main.rs:133, crates/defuddle-cli/src/main.rs:139
- **Problem:** File::create opens and truncates --output before the property name is validated; an unknown property then returns an error. With a scratch file containing nine bytes, piping a valid article into `defuddle --property typo --output "$scratch"` exits 1 with `unknown property` and leaves the file at zero bytes.
- **Recommendation:** Validate the property and compute the selected output before opening the destination; add a CLI regression asserting a pre-existing destination survives invalid input.
- **Fix with:** error-model
- **Effort:** S

