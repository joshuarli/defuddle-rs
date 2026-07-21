# Stage 1 baseline

Recorded 2026-07-21 with the checked-in vendored fixtures, release benchmark profile, 10 warmup iterations, and 100 measured iterations. The benchmark command is:

```bash
cargo bench -p defuddle --bench baseline
```

The harness validates the metadata preamble and Markdown golden for every case, and validates the HTML golden when one is present. `MiB/s` is input bytes divided by median extraction time; it is a throughput indicator, not a claim about output bandwidth.

| Case | Input bytes | HTML bytes | Markdown bytes | HTML median | HTML MiB/s | Markdown median | Markdown MiB/s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Hacker News listing | 5,091 | 1,290 | 1,100 | 318 µs | 15.23 | 1.95 ms | 2.49 |
| Hacker News post | 2,609 | 580 | 232 | 118 µs | 21.03 | 529 µs | 4.70 |
| Hacker News nested comments | 3,438 | 1,990 | 872 | 247 µs | 13.23 | 1.77 ms | 1.85 |
| Ordinary article (Wikipedia) | 30,183 | 16,603 | 8,772 | 39.32 ms | 0.73 | 53.75 ms | 0.54 |
| Extractor-free document | 1,251 | 1,109 | 587 | 1.90 ms | 0.63 | 3.94 ms | 0.30 |

The baseline is correctness-complete for the selected cases: all five Markdown comparisons passed, and the extractor-free case also matched its checked-in HTML golden. The benchmark uses a custom `harness = false` bench, so the same command is both the repeatable measurement entry point and the correctness gate for these cases.

## Post Stage 3 verification

Recorded after the DOM-backed Hacker News adapter migration, using the same command and iteration counts. All five Markdown comparisons and the available HTML golden comparison passed.

| Case | Input bytes | HTML bytes | Markdown bytes | HTML median | HTML MiB/s | Markdown median | Markdown MiB/s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Hacker News listing | 5,091 | 1,325 | 1,100 | 311 µs | 15.57 | 2.02 ms | 2.40 |
| Hacker News post | 2,609 | 549 | 232 | 124 µs | 20.01 | 544 µs | 4.57 |
| Hacker News nested comments | 3,438 | 1,930 | 872 | 270 µs | 12.13 | 1.83 ms | 1.80 |
| Ordinary article (Wikipedia) | 30,183 | 16,603 | 8,772 | 39.48 ms | 0.73 | 51.14 ms | 0.56 |
| Extractor-free document | 1,251 | 1,109 | 587 | 1.88 ms | 0.63 | 3.80 ms | 0.31 |
