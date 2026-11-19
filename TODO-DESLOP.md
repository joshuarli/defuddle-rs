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

**Strict-profile trial**

At `9edcdb27`, Clippy 0.1.101 on nightly 2026-11-09
checked all workspace targets. The table counts distinct
locations in the three first-party crates, including inline
and separate test code. Vendored htmd keeps its own policy.

| Candidate | Locations | Decision |
| --- | ---: | --- |
| `redundant_clone` | 4 | Adopt for first-party crates. Each diagnostic identifies a value that can be moved. |
| `unsafe_code` | 0 | No current violation. |
| `match_wildcard_for_single_variants` | 0 | No current violation. |
| `wildcard_enum_match_arm` | 13 | Reject. Examples include intentional fallbacks for node kinds that the operation skips. |
| `cast_possible_truncation` | 2 | Reject. Elapsed milliseconds would need to exceed the range of u64. |
| `cast_sign_loss` | 0 | No current first-party violation. |
| `unwrap_used` | 41 | Reject. The run includes generated constants, fixed selectors, and test assertions. |
| `expect_used` | 30 | Reject. The run includes fixed patterns and test assertions. |
| `needless_pass_by_value` | 2 | Reject at this scope. The trial found private argument borrowing opportunities without a demonstrated contract error. |

The candidate moves the already-owned HTML namespace, fallback
Markdown, author text, and schema text. The public types stay the
same. All 161 local workspace tests passed, including the
fixture corpus, JSON-LD metadata cases, CLI cases, and the native
WASM wrapper test. Strict Clippy passed.

The fixture-corpus gate passed on Mac17,7 with macOS 27.2
and nightly 2026-11-09. The exact comparison used baseline
`9edcdb27becee47c5fe367b5c177953afdd25753` and candidate
`b67578c20dafd8d369d8a9ccf1b3b0b1da370c26`. Both release
executables used the locked Rustybench 0.5.0. Each timed pass
parsed 168 HTML inputs and consumed the returned content.
Five interleaved pairs each recorded five samples per side.
All ten before and after pair checks passed. Background
VM work remained active, and candidate ranges include outliers.

| Output | Baseline median ms (range) | Candidate median ms (range) |
| --- | ---: | ---: |
| HTML | 160.037 (155.986–172.481) | 161.034 (154.717–204.903) |
| Markdown | 194.460 (185.089–212.073) | 189.036 (183.161–241.847) |

Both wall-time medians stayed within baseline variation. Peak
tracked allocations were unchanged at 1,054,422 bytes for HTML
and 2,007,678 bytes for Markdown. Bench executable sizes were
3,838,960 and 3,839,584 bytes. [Reproduce](docs/performance.md)

**Test tiers at the Types baseline**

The local workspace run at `f20fa5c1` reported 161 tests.
There were 87 core unit tests, 30 API integration cases, nine
metadata cases, and four fixture-harness cases over 168 captured
inputs. The CLI had one unit test and 15 binary tests. The native
WASM wrapper had three tests, and vendored htmd had eight unit
tests. Four doctests ran. The fixture and CLI suites took about
0.92 s and 0.74 s respectively after compilation. The fixture-corpus
benchmark remains the separate timed workload.

The CLI checks now compare emitted JSON fields, exact Markdown,
and the replaced destination file. They exercise implicit stdin,
a dash, and the empty source argument. The empty TTY case was
removed. With the ordinary Markdown write changed to emit zero
characters, the CLI exited successfully and the exact stdout case
failed. Restoring the write passed all 160 tests and strict Clippy.
A real TTY fixture and the retained mutation gate remain open
under DSLP-4.

**Workspace branch coverage**

`make coverage` runs the locked workspace with cargo-llvm-cov 0.9.1
and the pinned nightly's LLVM tools. Use
`cargo llvm-cov report --branch --json --summary-only` for the
machine-readable totals. The macOS ARM64 comparison uses the
Types default `f20fa5c1` and the CLI checkpoint `57014178`.
The instrumented suite ran 157 and 156 tests respectively,
with vendored htmd included and doctests outside this report.

Both runs covered 12,134 of 14,559 lines. Workspace branch
coverage moved from 2,995 to 2,996 of 4,660. The CLI's entry
module moved from 48 to 49 of 60 branches, with 229 of 301 lines
covered on both revisions. The eight-case stdin and option
forwarding mutation scope is separate.

**Tests inventory at `eebba91a`**

| Public boundary and consumer | Critical decisions and existing checks | Open evidence |
| --- | --- | --- |
| `parse_html` and `parse_html_profiled`, used by the CLI and WASM adapter | DOM preparation, short-result retry, extractor selection, sanitize before output, phase reporting. `integration_tests` exercises the public results. `fixture_harness` checks 168 captured inputs. | Select and disposition the critical decision regions. DSLP-6 tracks retry pruning. |
| Extraction metadata used by both wire consumers | Document, Open Graph and JSON-LD precedence, author, dates, language and schema text. `metadata_tests` checks these values and corpus preambles. | Metadata precedence and adapter selection have no mutation results yet. |
| CLI on macOS ARM64 and hosted Linux ARM64 | Source and option admission, SingleFile size limit, dispatch to the library, JSON and Markdown output, property handling and destination replacement. `cli_tests` runs the executable built by Cargo. | Real terminal stdin, exact error statuses and property failure before opening a destination (DSLP-7). The focused mutation scope covers only stdin selection and option forwarding. |
| `parse_page` and `list_extractors` for WASM callers | Options to extraction, JSON result, and extractor enumeration. Three native adapter tests cover successful output and listing. | A real WASM browser run and the error boundary remain unverified. |

Run the ordinary suite with `make test`, strict lint with
`make lint` and the coverage report with `make coverage`.
Recheck the fixture-corpus budget only if extraction changes.
The retained mutation command, whole-change selection,
executed CI controls and paired gate cost remain open.

## Tickets




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

