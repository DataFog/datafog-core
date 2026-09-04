# Structured PERSON implementation: verification and measurements

Recorded 2026-09-04 for the local implementation of
[ADR 002](adr/002-structured-person-discovery.md). Packages are not published.
The baseline is commit `9db22844f6828a4f592c3aed82c75475710e867b`.

## Verification

- `cargo fmt --all --check` passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed.
- `cargo test --workspace --all-features` passed: 76 tests.
- Installed Python wheel, packed Node package, and packed browser WASM package
  passed their existing text fixtures and the shared structured fixtures.
- Node and WASM consumer TypeScript checks cover the new public APIs; the WASM
  runtime suite runs in Chromium.
- Mintlify build validation and public-document link checks passed; relative
  links in the engineering plan, ADR, and measurement notes also resolve.
- Positive and negative aliases, explicit mappings, exclusions, JSON Pointers,
  Unicode ranges, invalid inputs, stateless protection, and special JSON keys
  are covered. Provider tests cover request validation before provider work,
  key deduplication, token batches, restoration, and scope rejection. Node also
  verifies that asynchronous operations use a snapshot of caller inputs.

There are no new third-party dependencies, downloaded dictionaries, or model
assets. Detection uses Core's finite field-alias rules and existing detectors.
The seven text-detector fixture expectations are unchanged.

## Environment and method

macOS on Apple Silicon; Rust/Cargo 1.88.0; Python 3.14.6; Node 24.19.0 for package
tests and structured timings. Raw native-import comparisons below use the same
Node 25.8.2 executable for both artifacts. All native artifacts are release
builds for macOS ARM64. These measurements do not cover other deployment targets.

The benchmark is [scan_benchmark.rs](../crates/core/examples/scan_benchmark.rs).
Run from the repository root:

```sh
cargo build -p datafog-core --release --example scan_benchmark
/usr/bin/time -l target/release/examples/scan_benchmark 10000
/usr/bin/time -l target/release/examples/scan_benchmark 10000 structured
```

The baseline used the same text-only benchmark before the structured branch
was added. To repeat against the baseline commit, copy the example to a separate
baseline checkout and omit its `if ... == Some("structured")` block; that block
references the new APIs. Keep the text corpus and measurement loop identical.

Text timings use 100 development fixtures, 6,606 UTF-8 bytes per iteration,
10,000 iterations, and 1,000,000 findings. Structured timings use the 13 input
documents from `fixtures/structured.jsonl`, including nested data, arrays,
Unicode names, empty values, and unrelated fields. Timing uses default discovery
and redaction for every document, independently of fixture-specific policies.
Core timings start with parsed values and exclude JSON parsing and bindings.

## Existing text scanner

| Measurement | Baseline | Candidate |
| --- | ---: | ---: |
| Repeated-scan elapsed time | 0.765740 s | 0.786930 s |
| Throughput | 82.273 MiB/s | 80.058 MiB/s |
| First scan in process | 857.125 µs | 2,161.458 µs |
| Maximum resident set size | 4,341,760 bytes | 4,554,752 bytes |
| Peak memory footprint | 3,064,144 bytes | 3,096,912 bytes |

These are single runs on a shared machine. The first-scan difference is not an
isolated startup-regression estimate; repeated throughput also has no measured
confidence interval. The evidence establishes behavior parity and records an
initial cost comparison. It does not establish a numerical performance SLA.

## Structured operations

| Mean time per small document | Rust Core | Python binding | Node binding |
| --- | ---: | ---: | ---: |
| Discover mappings | 0.545 µs | 3.008 µs | 2.454 µs |
| Discover and scan | 0.953 µs | 3.428 µs | 4.618 µs |
| Discover, scan, and redact | 4.353 µs | 10.634 µs | 11.534 µs |

Core used 10,000 iterations over the 13 documents. It returned 140,000 mapping
records, 180,000 findings, and 160,000 replacements respectively. The structured
benchmark process reached 4,653,056 bytes maximum RSS. No allocation-count
profiling was performed.

Bindings used 1,000 iterations over the same parsed documents, calling
`discover_fields`/`discoverFields`, `scan_structured`/`scanStructured`, and
`scan_and_transform_structured`/`scanAndTransformStructured` with default
redaction. Timings include runtime validation, JSON transport, Core work, and
result conversion. They exclude package import, file loading, and provider I/O.
These are end-to-end binding costs, not an isolated measurement of serialization.

## Native import and artifact size

| Artifact | Baseline bytes | Candidate bytes | Change |
| --- | ---: | ---: | ---: |
| Python extension | 3,578,112 | 4,167,472 | +589,360 (+16.5%) |
| Node native module | 2,960,192 | 3,222,832 | +262,640 (+8.9%) |
| Browser WASM | — | 1,943,549 | Comparable clean baseline not captured |

Sizes are uncompressed native payloads, not wheel/tarball transfer sizes or
installed footprints. Python and Node include the whole new structured API,
transformation records, and provider coordination, as well as discovery.

Five fresh subprocesses loaded each baseline/candidate native extension from
separate directories. Python measured `import datafog_core` with
`time.perf_counter()`; Node measured `require("./datafog.node")` with
`performance.now()`. The median of all five samples was:

| Native import | Baseline | Candidate |
| --- | ---: | ---: |
| Python | 0.656 ms | 0.726 ms |
| Node | 1.695 ms | 1.884 ms |

The filesystem and OS loader caches were not cleared. First loads of copied
candidate binaries took 161.326 ms (Python) and 123.274 ms (Node), versus 8.615 ms
and 9.084 ms for previously loaded baseline copies. Earlier initial baseline
loads took 231.823 ms and 218.047 ms. This variation makes these measurements
unsuitable for a cold-start guarantee. Native-import timings omit the JavaScript
wrapper and Python runtime startup.

## Open claims and release work

No product latency or package-size budget has been established. Large payloads,
production-schema precision/recall, browser startup, other architectures, and
provider latency remain unmeasured. Synthetic field fixtures cannot establish
general person-name recognition in prose.

Release/version selection, publication, and migration of the current MCP
consumer remain pending. The current MCP repository must be identified before
its customer-record flow can be tested against these APIs.
