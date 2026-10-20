# Performance

Performance baselines time the public `parse_html()` pipeline over all vendored
upstream HTML fixtures. The benchmark loads its inputs before timing and
measures one sequential pass over the 168 fixtures, either with HTML output
only or with separate Markdown output. Fixture URLs use the same helper as the
fixture parity harness.

## Run the benchmark

Install the pinned runner, then capture five measured runs with five samples
per run:

```sh
cargo install rustybench --version 0.4.0 --locked
rustybench baseline \
  --root . \
  --baseline benchmarks/macos-aarch64-18-48gb-fixtures-2026-10-20.json \
  --warmup 1 --runs 5 --sample-count 5 --sample-size 1 -- \
  cargo bench -p defuddle --bench fixture_bench
```

Each sample includes parsing every fixture and consuming the returned HTML and
Markdown byte counts. File reads and benchmark compilation are outside the
timed region. Run the fixture parity check separately when changing extraction
behavior:

```sh
cargo test -p defuddle --test fixture_harness
```

Set `DEFUDDLE_PHASES=1` to run an instrumented Markdown pass and print time by
pipeline phase. Use it to identify bottlenecks; phase timings include
instrumentation and are not comparable to benchmark medians.

```sh
DEFUDDLE_PHASES=1 cargo bench -p defuddle --bench fixture_bench
```

## Baselines

[`benchmarks/macos-aarch64-18-48gb-fixtures-2026-10-20.json`](../benchmarks/macos-aarch64-18-48gb-fixtures-2026-10-20.json)
records the first baseline on a Mac17,7 with 18 logical CPUs and 48 GiB of
memory, running macOS 27.0.1 and Rust 1.100.0-nightly. It processes 168
fixtures with 1,130,865 input HTML bytes. Each value is the median of five runs
with five samples per run and one corpus pass per sample.

| Mode | Median per pass | Run median range | Allocations | Cumulative allocated bytes | Peak live bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| HTML only | 247.56 ms | 243.55–255.07 ms | 1,341,905 | 477,940,593 | 1,623,214 |
| HTML and Markdown | 294.21 ms | 289.52–295.92 ms | 1,534,764 | 503,031,487 | 2,566,108 |

The fixture pass consumes the parsed HTML and Markdown byte counts. A separate
instrumented Markdown pass measured content-pattern removal at 18.1% of phase
time, selector removal at 17.1%, standardization at 13.5%, and generic scoring
at 12.1%. Phase instrumentation changes timings, so use these shares to find
work to inspect, not to compare benchmark revisions.

Use this file as the baseline for this fixture corpus on this machine. For
future comparisons, keep the fixtures, Rust toolchain, Rustybench version,
allocator, and options fixed. Report the median and run-to-run range. Treat a
difference inside that range as inconclusive. Keep performance changes generic
across HTML inputs rather than keying them to a fixture, site, or URL.
