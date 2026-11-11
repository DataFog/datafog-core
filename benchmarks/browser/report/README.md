# Receipt analysis and report

Keep pilot and final JSONL receipts in separate directories. The analyzer requires `runKind` on each receipt and an explicit mode selection, so it cannot mix pilot results into final summaries. It validates receipt shape, records admission and correctness failures, rejects duplicate trial IDs and reused profile IDs within a scenario/run, and excludes non-admitted, correctness-failing/unsupported, or explicitly excluded trials from latency summaries.

Percentiles are linear-interpolated empirical quantiles from one duration per independent eligible profile. The predeclared reporting floor is 100 independent eligible profile trials per scenario/backend/interval; p50 and p95 remain blank below that count, while n, failure rates, exclusions, and measurement unavailability remain visible. This floor supports five points in the upper five-percent tail; it is not a performance acceptance threshold or a confidence interval.

Analyze selected final receipts:

```sh
python3 benchmarks/browser/report/analyze.py \
  --run-kind final \
  --output-dir target/browser-benchmark/report-final \
  target/browser-benchmark/receipts/final/*.jsonl
```

Analyze pilot data independently by replacing `final` with `pilot` and selecting only pilot receipt files. Outputs are `summary.json`, `timings.csv`, `report.md`, and `latency.svg`. The report separates failures (admission rejection or failed correctness gates), correctness-unsupported trials, and explicit exclusions; all remain visible and none enter latency percentiles. Complete the context fields in the generated Markdown with package hashes, harness commit, browser/hardware coverage, backend security settings, measurement limitations, and workload-specific conclusions. Do not write conclusions where receipts lack coverage or percentile sample size.

Malformed receipts fail analysis instead of being ignored. Raw JSONL receipts are never changed by the analyzer.
