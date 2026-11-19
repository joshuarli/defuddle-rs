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
The retained mutation gate remains open under DSLP-4.

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
| CLI on macOS ARM64 and hosted Linux ARM64 | Source and option admission, SingleFile size limit, dispatch to the library, JSON and Markdown output, property handling and destination replacement. `cli_tests` runs the executable built by Cargo, and `cli_tty` exercises terminal stdin. | Wider exact error statuses and mutation scopes for archive and output handling. |
| `parse_page` and `list_extractors` for WASM callers | Options to extraction, JSON result, and extractor enumeration. Four tests run natively and in Node's WASM runtime. Chromium verifies the generated web bindings and iframe removal. | Error translation and a retained browser gate. |

Run the ordinary suite with `make test`, strict lint with
`make lint` and the coverage report with `make coverage`.
Recheck the fixture-corpus budget only if extraction changes.
The retained mutation command, whole-change selection,
executed CI controls and paired gate cost remain open.

**CLI option forwarding and stdin selection**

At the Types default `f20fa5c1`, the CLI suite caught five of
eight cases. At `57014178`, exact Markdown output caught the
deletion of the Markdown option passed to the library. The
debug selector and Gmail override cases caught the remaining
two option deletions at `3b6addc0`. The three lists of
mutation identities match. The final result was eight caught,
with zero misses, timeouts or unviable builds.

Use cargo-mutants 27.1.0 on macOS ARM64 with nightly 2026-11-09.
The first two runs used disposable copies. The last reused a
warm target with `--in-place` in an isolated checkout. These
runs establish outcomes for the matched scope, with different
build conditions. Run the command in disposable scratch to
reproduce the eight identities and tests.

```sh
cargo mutants -p defuddle-cli --file crates/defuddle-cli/src/main.rs \
  --re 'main\.rs:(119|122|201):' --baseline run --timeout 20 \
  --build-timeout 300 --cargo-test-arg=--test --cargo-test-arg=cli_tests
```

**Destination preservation**

At `8ea4e261`, a real CLI run with `--property typo` exited
with status 1 and the expected diagnostic. The file under
`--output` was truncated. Resolving the property before
opening the destination preserves its existing bytes and
keeps the input diagnostic. All 17 binary cases and strict
CLI Clippy passed. This closes DSLP-7.

**Terminal stdin**

The Unix-only `cli_tty` test runs the built binary with a
real terminal and no source. It requires the missing-input
diagnostic and exit code 1 within five seconds. Disabling
the terminal guard in scratch left the process waiting for
input and the test failed at its deadline. Restoring the
guard passed all 19 CLI tests.

`laputa-ptytest` 0.4.0 is an MIT-licensed Unix test
dependency. The lock adds the harness and four terminal
dependencies. The existing locked versions and production
dependency graph were preserved.

**Document-wide pruning on retries**

At `c18db995`, the short-article fixture triggered both generic
retries and the selected result restored the removed footer.
Both retries now reapply document-wide pruning when the initial
parse removed content. The exact output contains only the
article paragraph. All 31 integration cases, four fixture-harness
cases and strict Clippy passed. This closes DSLP-6.

The fixture-corpus benchmark compared the unchanged extraction
baseline at `c18db995` with the extraction patch at `92142446`.
The baseline executable was reused from the Types run after
checking the exact library, fixture and lock inputs. On macOS
ARM64 with nightly 2026-11-09 and Rustybench 0.5.0, all five
interleaved pairs passed readiness. Each side used five samples
per pair over the same 168 fixtures.

| Output | Baseline median ms (range) | Candidate median ms (range) |
| --- | ---: | ---: |
| HTML | 200.527 (190.409–209.445) | 198.134 (181.118–215.365) |
| Markdown | 232.056 (217.901–252.190) | 233.963 (224.076–248.748) |

Both medians stayed within baseline variation. The median live
peak was unchanged at 1,054,422 bytes for HTML and 2,007,678
bytes for Markdown. Median cumulative allocated bytes rose by
about 11 KiB per pass. The benchmark executables were
3,839,584 and 3,839,568 bytes. [Reproduce](docs/performance.md)

**SingleFile archive manifest**

The stored-entry fixture supplies both the default index and
a manifest-selected article. The CLI emits the exact chosen
title and content and uses the archived URL unless the caller
supplies `--url`. The existing Deflate file fixture and size
limit case still run.

Two scratch controls at `6c32f883` independently changed the
manifest index lookup and URL selection. The new case observed
the default index in the first control and the archived domain
in the second. Both runs failed their exact assertions.
Restoring the source passed all 18 binary cases and strict
CLI Clippy.

**Favicon metadata**

At `557757c9`, a page with only a shortcut icon returned
an empty favicon. The selector parser split the quoted
attribute at the space. The extractor now reads the exact
relationship value from the link element. The public cases
check the declared icon priority, unchanged relative icon
paths, HTTP and HTTPS origin fallback, and absent URLs.
All 11 metadata cases, four fixture-harness cases and strict
Clippy passed.

Cargo-mutants 27.1.0 selected nine identities in the original
`get_favicon` with `metadata_tests`. Four were caught, three
survived and two were unviable. The fixed source at `aa9ecff1`
selected ten after adding the explicit relationship check.
All eight viable identities were caught. The two let-chain
operator variants were unviable in both runs.

Five readiness-qualified fixture-corpus pairs on macOS ARM64
compared the release executables for the same 168 inputs.
The HTML medians were 169.066 and 169.425 ms, with a baseline
range of 161.219 to 239.967 ms. The Markdown medians were
198.863 and 193.296 ms, with a baseline range of 189.546 to
252.273 ms. Both candidate medians stayed within baseline
variation. Peak live allocation stayed at 1,054,422 and
2,007,678 bytes. The executable sizes were 3,839,568 and
3,838,720 bytes. [Reproduce](docs/performance.md)

**WASM runtime and web bindings**

At `b08b13e0`, the WASM harness executed one adapter test.
All four adapter tests now pass on the native host and in
Node 26.12.0 with wasm-bindgen-test-runner 0.2.130. They
exercise extracted output, exact Unicode JSON and extractor
enumeration.

The generated web bindings passed the loopback fixture in
Chromium 155.0.8059.84. The fixture checks exact HTML and
Markdown, domain and title, iframe removal, and adapter names.
Turning off iframe removal in an isolated build left the
iframe in the generated HTML and failed the browser fixture.
The source was restored byte for byte and rebuilt. Strict
adapter Clippy passed.
[Reproduce](docs/testing.md#wasm-runtime)

## Tickets




### DSLP-4 · P2 · Tests: coverage, boundary gaps, mutation

- **Area:** deslop · tests
- **Where:** test suites and CI
- **Problem:** Workspace coverage, the eight CLI option-forwarding and stdin mutation identities, and the real terminal input boundary are accounted for. Critical extraction, metadata, archive, output and WASM decisions still need disposition. The whole-change gate and measured cost remain open.
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
