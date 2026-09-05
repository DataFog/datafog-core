# Slice Ten: finding-selection performance

This change addresses [issue #10](https://github.com/DataFog/datafog-core/issues/10)
in the shared Rust transformation path, following the structured PERSON work.
Public APIs, policies, detection coverage, offsets, and package versions are
unchanged. PERSON and this optimization are intended for a combined 0.3.0
release after review; this change does not publish packages.

## Algorithm and compatibility

Every supplied finding is still validated before entity filtering or allowlists,
including findings that will ultimately be discarded. The first invalid finding
retains its original index and error. Selection then:

1. Collapses duplicates through a `BTreeMap` keyed by entity type and byte range.
   Validation against the same input ensures that matched text and code-point
   range are also identical. Each duplicate group keeps the original encounter
   order and confidence/provenance preference.
2. Sorts by source position. If all intervals are disjoint or merely touching,
   returns them directly.
3. For sortable preferences, sorts once by the existing overlap priority and
   accepts each candidate only if it misses the nearest accepted interval on
   either side. A `BTreeMap` provides logarithmic lookup and insertion; results
   are returned in source order.

For `m` supplied findings, duplicate indexing takes `O(m log m)` comparisons.
Overlap selection takes `O(u log u)` comparisons for `u` unique candidates in
the sortable case, with an `O(u)` disjoint check after source sorting. Additional
index storage is `O(m)`. These bounds exclude validation, filtering, text cloning,
and variable-length string comparison costs.

The existing preference is **not always a total order**: confidence is compared
only when both findings provide it. For equal-length spans on `abcdef`, let
`A = [0,4), confidence 0.2`, `B = [1,5), no confidence`, and
`C = [2,6), confidence 0.8`. A beats B by position, B beats C by position, and C
beats A by confidence. Sorting that cycle could change the protected output.

When any overlaps exist and an equal-code-point-length group mixes present and
absent confidence, selection conservatively keeps the original pairwise overlap
algorithm. This fallback remains `O(u²)` in the worst case. Different lengths
can safely use different confidence-presence modes because length takes priority.
All current built-in detectors, including structured PERSON, omit confidence
and therefore use the faster algorithm. Disjoint inputs take the fast path
regardless of confidence. Duplicate preferences can also be cyclic, which is why
indexing preserves the original per-group fold rather than sorting duplicates.

## Policy rationale and comparable tools

The fallback preserves the confidence rule in
[ADR 001](adr/001-privacy-core-contract.md#duplicates-and-overlaps). It is a
compatibility choice for this performance change, not a requirement for every
future overlap policy. The public
[finding guide](concepts/findings-and-ranges.mdx#missing-confidence) explains
what callers supplying optional confidence should expect.

The following comparison was checked on 2026-09-04. These are policy examples;
they do not establish equivalent outputs or performance across the tools.

| Tool | Confidence and overlap policy | Relevance to Core |
| --- | --- | --- |
| Presidio | Recognizer results expect a numeric score, and regex patterns require an assigned score. Its anonymizer prefers higher scores for identical spans and the larger span for containment. It also merges overlapping findings of the same type and handles partial intersections differently from Core. | A score can be assigned by a rule author without running a model. The supported contract avoids our distinction between scored and unscored findings, but adopting the full policy would change Core behavior. |
| spaCy EntityRuler | Among overlapping rule matches, prefers the span with more tokens, then the earlier position. Confidence is not used for this selection. | Structural priority avoids the missing-confidence cycle. Core measures length in Unicode code points, so its ranges and length metric would remain different. |
| Google Sensitive Data Protection | Uses defined likelihood levels. A custom detector defaults to `VERY_LIKELY` when its likelihood is omitted. Explicit exclusion rules can suppress domain matches that overlap email matches. | Defaults give omitted configuration a defined meaning, and explicit rules handle particular overlap relationships. This does not establish Google's complete internal winner-selection algorithm. |

Sources:

- Presidio: [result score contract](https://github.com/data-privacy-stack/presidio/blob/e9b5795ff9302fc8a306eee9a73b87ab00426166/presidio-analyzer/presidio_analyzer/recognizer_result.py),
  [regex pattern scores](https://presidio.dataprivacystack.org/tutorial/02_regex/),
  [documented overlap behavior](https://presidio.dataprivacystack.org/anonymizer/#handling-overlaps-between-entities),
  and [same-type merging implementation](https://github.com/data-privacy-stack/presidio/blob/e9b5795ff9302fc8a306eee9a73b87ab00426166/presidio-anonymizer/presidio_anonymizer/anonymizer_engine.py).
- spaCy: [EntityRuler overlap rules](https://spacy.io/api/entityruler#call).
- Google: [custom-detector likelihood default](https://docs.cloud.google.com/sensitive-data-protection/docs/reference/rest/v2/InspectConfig#CustomInfoType)
  and [overlap-exclusion example](https://docs.cloud.google.com/sensitive-data-protection/docs/samples/dlp-inspect-string-without-overlap).

### Proposed future direction — not implemented

For a future simplification, prefer a consistent structural ordering: longer
Unicode code-point span, earlier source position, then stable entity/provenance
tie-breakers. Retain confidence as metadata without using it to rank overlaps.
This would keep confidence optional while allowing all overlap candidates to be
sorted consistently, including mixed-confidence inputs.

This proposal would change some winners for caller-supplied scored findings.
It needs a separate behavior decision and focused PR updating ADR 001, public
documentation, and regression fixtures. That work must also explicitly decide
whether confidence still chooses provenance within duplicate groups; changing
overlap ordering alone does not remove their encounter-order dependence.
No confidence policy option, inferred score, or change to protection behavior
is introduced in Slice Ten. The fallback stays in place for this PR.

## Reproducible measurement

Run from the repository root:

```sh
cargo test -p datafog-core --release finding_selection_benchmark -- --ignored --nocapture
```

The benchmark and differential tests live in
[selection_tests.rs](../crates/core/src/selection_tests.rs). The reference copies
the selection algorithm from commit `4275002833b45d846fcc75f3a8cd083310f89970`;
both versions use the unchanged preference comparators and run in the same
release build. Inputs are prevalidated. Timing includes policy filtering,
duplicate collapse, overlap selection, result allocation, and destruction. It
excludes scanning, range validation, replacement generation, bindings, and
provider I/O. No public benchmarking API or dependency was added.

Recorded on macOS ARM64 with Rust 1.88.0, 2026-09-04. Each implementation is
warmed, then measured seven times with alternating execution order; values below
are medians. Exact selected findings are compared before timing. Workloads use
unscored four-character spans: disjoint intervals; clusters of four partially
overlapping intervals (half survive); and groups of four duplicates (one quarter
survive). These are synthetic local measurements, not a product latency SLA.

| Workload | Input findings | Previous selection (µs) | New selection (µs) | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Disjoint | 256 | 461.875 | 52.042 | 8.88× |
| Disjoint | 512 | 1,527.041 | 97.541 | 15.66× |
| Disjoint | 1,024 | 3,557.000 | 133.750 | 26.59× |
| Disjoint | 2,048 | 15,150.042 | 261.750 | 57.88× |
| Disjoint | 4,096 | 60,289.583 | 561.000 | 107.47× |
| Overlap clusters | 256 | 190.834 | 36.625 | 5.21× |
| Overlap clusters | 512 | 734.667 | 88.542 | 8.30× |
| Overlap clusters | 1,024 | 2,904.125 | 180.917 | 16.05× |
| Overlap clusters | 2,048 | 12,108.334 | 419.417 | 28.87× |
| Overlap clusters | 4,096 | 51,017.417 | 877.375 | 58.15× |
| Duplicates | 256 | 41.583 | 13.250 | 3.14× |
| Duplicates | 512 | 139.083 | 27.750 | 5.01× |
| Duplicates | 1,024 | 562.916 | 60.583 | 9.29× |
| Duplicates | 2,048 | 2,206.708 | 142.792 | 15.45× |
| Duplicates | 4,096 | 8,114.958 | 313.625 | 25.87× |

Doubling disjoint findings from 2,048 to 4,096 took 3.98× as long previously
and 2.14× with this change. The algorithm establishes the complexity bound;
these measurements demonstrate its effect on the tested inputs.

## Correctness evidence and limits

Five additional regression tests cover the confidence cycle, order-sensitive
duplicates, validation before filtering/deduplication, interval boundaries, and
3,000 seeded randomized Unicode cases with three shuffled input orders each.
All 9,000 randomized comparisons preserve exact findings and provenance across
absent, present, mixed, and length-dependent confidence; entity filters; and
exact/regex allowlists. Existing transformation and provider tests cover output
ranges, policy application, and validation before provider calls.

Required Rust formatting, Clippy, and workspace tests pass: 81 tests, plus the
separately run manual benchmark. Installed Python, Node, and browser WASM
packages also pass the existing text, structured, and transformation fixtures.

The mixed-confidence fallback is intentionally still quadratic. Repeated Unicode
prefix walks during validation and output-range construction, repeated validation
across operation layers, and binding conversions are separate costs. This PR
does not establish an end-to-end `O(m log m)` bound or promise the selection-only
speedups for complete requests. Optimizing those costs requires separate evidence
and a separate focused change.
