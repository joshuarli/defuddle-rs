Prefix: DSLP
Next: 11

Deslop campaign backlog ([plan](https://github.com/laputa-systems/stuff/blob/main/docs/campaigns/deslop.md), tier B). One agent works this file with the `deslop` skill: take the lowest-numbered open ticket whose area does not start with `Needs owner ·`. After DSLP-5, work the open P1 and P2 findings, then fill in `## Report` below.

## Report

DSLP-1 measured default revision `63a23ca89d409d2e211991d5bd8ebfc1dc349716`;
its first parent is `9b165ada4cd93ff116b7c592413420ee3351aa61` and its
second parent is `0c84fcf21709cb9894b569efccbf79c068da243b`. It is an
ancestor of the report admission revision `4e8bfb1b21f339ab9afe3f11ff224d7d4e04b17b`.
Merged PRs #15–#32 and #34–#36 retain the detailed evidence and history.

**Scorecard**

| Measure | Before | After / disposition |
| --- | --- | --- |
| First-party Rust source lines | 17,754 | 17,756 |
| Separate test file lines | 2,757 | 3,245 |
| Warm `make test` | 4.55 s at Ground | 5.43 s after #34 (7.39 s paired baseline); contended host |
| Coverage, lines | 12,134/14,559 at Types | 12,223/14,590 after #34 |
| Coverage, branches | 2,995/4,660 at Types | 3,029/4,666 after #34 |
| Escape-hatch lines | 201 | 232; includes test assertions and inline tests |
| Dead code | unmeasured | #35 removed four private methods, 44 lines; no dependencies removed; private-module visibility narrowed; one analysis allowlist retained |
| Findings | DSLP-6 and DSLP-7 filed P2 | both fixed; three P3 tickets remain (DSLP-8–10); no open P1/P2 |
| Performance | Ground host busy | qualified Types, retry and favicon comparisons within observed spread; overall gate deferred: host busy; #33 parked, #36 adds entrypoint |
| Escaped regressions | not measured | no additional regression demonstrated by cumulative review; serializer error path remains unexercised |

Line counts are physical lines in tracked `crates/defuddle{,-cli,-wasm}/src/**/*.rs`
and separate `tests` Rust, Python and HTML files, excluding vendored htmd;
inline tests count as source. Escape hatches count matching lines in those
same files with the deslop Rust expression. Coverage includes vendored htmd;
its checkpoints are different revisions, not a controlled single-change comparison.

Mutation results describe bounded trials, not a repository-wide score. CLI
option forwarding and stdin: 5 caught / 3 missed / 0 timeout / 0 unviable
before, 8 / 0 / 0 / 0 after. Favicon: 4 / 3 / 0 / 2 before, 8 / 0 / 0 / 2
after, with changed identity sets. Redirects: 8 caught and 3 missed before,
10 caught and 1 missed after. URL precedence: 6 caught and 1 unviable both
times. Timeout counts for these latter scopes were not recorded. The unreadable-input
hand fault survived the original CLI suite and was caught by the new boundary
test, with no timeout or unviable fault; 110.72 s baseline plus 9.78 s control,
about two runner-minutes. These separate scopes must not be added into a single
mutation rate. Eight boundary groups gained evidence: option forwarding,
stdin/output, destination preservation, terminal input, archive manifests,
retry/redirect behavior, metadata, and WASM. Four user-visible defects were
fixed: invalid-property truncation, retry pruning, shortcut favicon recognition,
and plain HTML misclassified as a SingleFile archive. The latter two were
discovered during test work rather than assigned a recorded priority.

**Steps and cost**

- Ground (#15): 161 tests; cold 945.43 s, warm 4.55 s; warm lint 0.36 s.
  Default tests and lint and the fleet workflow rule passed; host readiness failed.
- Diagnose (#16): reproduced and filed P2 retry pruning and output truncation
  (DSLP-6 and DSLP-7); total wall and CI minutes were not recorded.
- Types (#17): adopted `redundant_clone` at four sites; rejected noisy
  candidates and retained public types. All 161 tests and strict Clippy passed;
  five readiness-qualified fixture pairs found no regression beyond spread.
  Total wall and CI minutes were not recorded.
- Tests (#18–#32, #34): exact CLI output and forwarding, terminal input,
  destination preservation, archive selection and errors, retry/redirect,
  metadata precedence and favicon, and native, Node and Chromium WASM
  boundaries gained tests and fault controls. Coverage and bounded mutation
  results are above. The hand fault cost 120.50 runner-seconds; total hosted
  trial and CI minutes were not recorded. `make test-wasm` covers runtime bindings;
  serializer error translation has no established reachable error case.
- Dead code (#35): compiler scans on native and wasm32 and cargo-machete
  found the four removable methods and no unused dependencies. Profiling
  snapshot machinery stays pending the owner decision in DSLP-8. Total wall
  and CI minutes were not recorded.
- Bench (#36): public `parse_html` corpus covers 168 captured pages; driver
  self-test is Edit and five samples are Change. Scenario pairs are manual;
  no CLI, archive or WASM workload, automated verdict, or process-tree memory
  budget exists (DSLP-9 and DSLP-10). The historical Rustybench 0.4.0 anchor
  is not directly comparable to the locked 0.5.0. #33 remains parked for
  readiness-qualified evidence. Total wall and CI minutes were not recorded.
- Report: reviewed the cumulative diff for extraction, CLI and WASM, including
  callers and boundary tests. No new demonstrated finding or ticket resulted.

**Method feedback**

Require destination-preservation assertions on every failure before opening an
output file: the invalid-property case escaped the initial CLI checks. Assert
exact outputs and exercise retries, real terminals and generated WASM bindings;
the faults showed those boundaries mattered. The quoted-space selector misfired
for `shortcut icon`; explicit attribute matching repaired it. The strict-profile
trial found no witness for several candidates; rejecting them avoided noisy
rules. Coverage alone did not expose the unreadable-input behavior.

**Shared-file gaps**

The stuff crate mirror does not serve cargo-mutants, so a retained mutation CI
lane was not qualified; the shared registry needs that package before the lane
can be established. Shared usr/stuff files were not edited.

## Tickets






### DSLP-8 · P3 · Decide whether to retire profiling snapshot machinery

- **Area:** Needs owner · profiling
- **Where:** crates/defuddle/src/lib.rs:525; crates/defuddle/src/analysis.rs
- **Problem:** Profiling constructs a structural snapshot and empty mutation overlay solely to report their named timings; overlay mutation operations have only unit-test consumers. Maintaining this machinery costs code and profiling work, but removing it changes the phases exposed by parse_html_profiled when profile is enabled.
- **Recommendation:** Decide whether consumers may lose the analysis snapshot and analysis overlay timing phases; if so remove the machinery and its isolated tests together.
- **Fix with:** debloat
- **Effort:** M

### DSLP-9 · P3 · Cover CLI and WASM performance journeys

- **Area:** benchmarks
- **Where:** docs/performance.md; crates/defuddle/benches/fixture_bench.rs
- **Problem:** The corpus measures library extraction only; CLI startup, input and output I/O, archive handling and WASM execution have no representative resource or latency evidence. No regression is demonstrated.
- **Recommendation:** Define representative completed-work workloads and correctness oracles for the public CLI and WASM journeys before setting budgets.
- **Fix with:** bench-pyramid
- **Effort:** M


### DSLP-10 · P3 · Strengthen corpus comparison and attribution evidence

- **Area:** benchmarks
- **Where:** crates/defuddle/benches/fixture_bench.rs; docs/performance.md
- **Problem:** The corpus consumes output lengths but does not assert expected outputs; comparisons and readiness are manual, with no automated regression verdict. Allocator counters omit RSS and process-tree memory, and phase timings do not attribute allocation, CPU samples or I/O. No regression is demonstrated.
- **Recommendation:** Tie correctness to the fixture oracle, capture reproducible interleaved comparisons and host validity, and document low-overhead CPU and allocation attribution separately from timing.
- **Fix with:** bench-pyramid
- **Effort:** M
