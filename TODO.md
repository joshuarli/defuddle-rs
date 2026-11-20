Prefix: DEFR
Next: 13

### DEFR-10 · P3 · Decide whether to retire profiling snapshot machinery

- **Area:** Needs owner · profiling
- **Where:** crates/defuddle/src/lib.rs:525; crates/defuddle/src/analysis.rs
- **Problem:** Profiling constructs a structural snapshot and empty mutation overlay solely to report their named timings; overlay mutation operations have only unit-test consumers. Maintaining this machinery costs code and profiling work, but removing it changes the phases exposed by parse_html_profiled when profile is enabled.
- **Recommendation:** Decide whether consumers may lose the analysis snapshot and analysis overlay timing phases; if so remove the machinery and its isolated tests together.
- **Fix with:** debloat
- **Effort:** M

### DEFR-11 · P3 · Cover CLI and WASM performance journeys

- **Area:** benchmarks
- **Where:** docs/performance.md; crates/defuddle/benches/fixture_bench.rs
- **Problem:** The corpus measures library extraction only; CLI startup, input and output I/O, archive handling and WASM execution have no representative resource or latency evidence. No regression is demonstrated.
- **Recommendation:** Define representative completed-work workloads and correctness oracles for the public CLI and WASM journeys before setting budgets.
- **Fix with:** bench-pyramid
- **Effort:** M

### DEFR-12 · P3 · Strengthen corpus comparison and attribution evidence

- **Area:** benchmarks
- **Where:** crates/defuddle/benches/fixture_bench.rs; docs/performance.md
- **Problem:** The corpus consumes output lengths but does not assert expected outputs; comparisons and readiness are manual, with no automated regression verdict. Allocator counters omit RSS and process-tree memory, and phase timings do not attribute allocation, CPU samples or I/O. No regression is demonstrated.
- **Recommendation:** Tie correctness to the fixture oracle, capture reproducible interleaved comparisons and host validity, and document low-overhead CPU and allocation attribution separately from timing.
- **Fix with:** bench-pyramid
- **Effort:** M

