# German PII implementation verification

This records pre-release source verification based on `origin/main` at `88c4fad`.
The German detectors subsequently shipped in Core 0.4.0; the measurements below
retain their original source baseline.
The focused locale suite is `fixtures/german.jsonl`. Expected German findings
are filtered by `DE_` because generic detectors may legitimately overlap them.
Its 304 cases cover aliases/defaults, accepted and rejected lexical/context
forms, original casing/separators, Unicode prefixes, repeated occurrences and
an ordered all-seven document. Authored source spans also specify redaction,
mask/removal, selection, override and exact/full-match regex allowlist outputs.

Rust (`crates/core/tests/german.rs`), installed Python
(`bindings/python/tests/german_conformance.py`), and installed Node/browser
(`scripts/german-conformance.mjs`) consume the same fixtures. Every row also
runs through two fields and an array element with escaped JSON Pointer paths
and string-local ranges. Dedicated cases reject cross-field/key context and
preserve malformed-config errors. Overlap regressions cover IBAN/card,
VAT/SSN, tax/PHONE (equal span), and postal/ZIP_CODE without changing selection.
The IBAN overlap fixture uses tabs around a valid-shaped 16-digit card; the
ordinary fully grouped IBAN example has no overlapping base finding.

Existing providers exercise pseudonymization and tokenization/restoration for
all seven labels in Rust, Python and Node. Browser tests check that provider
strategies remain unsupported. No binding detection logic or public type shape
changed: labels and locale are already strings in every binding declaration.
Existing development/final and structured fixture files remain unchanged.

## Checks run

On macOS arm64, Rust 1.88.0:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`: 85 unit and 4 integration tests pass;
  one pre-existing performance test remains ignored.
- Built and installed the abi3 wheel with maturin; installed-package tests pass
  on CPython 3.14.6. Both mypy and pyright typing suites pass.
- Built/packed/installed Node package: fixture, transformation, provider and
  TypeScript tests pass on Node 24 (also exercised on local Node 25).
- Built/packed/installed WASM package: TypeScript and real Chromium package tests
  pass, including the full German suite.

The local Xcode license prevented Apple's default tool dispatch. Commands used
`DEVELOPER_DIR=/Library/Developer/CommandLineTools`; no license acceptance or
system setting changes were needed. CI covers the broader Python version matrix;
local verification does not claim that entire matrix was executed.

## Performance

`crates/core/examples/german_benchmark.rs` contains four synthetic workloads.
The same example was compiled in release mode against baseline `88c4fad` and
this implementation. Seven paired trials alternate binary order, 1,000 scans
per workload/configuration per trial. Each configuration is warmed before timing.
The table reports median microseconds per scan; these are local observations,
not stable CI thresholds or cross-machine capacity estimates.

| Workload | UTF-8 bytes | Baseline, no locale | Branch, no locale | Change | Branch, German |
| --- | ---: | ---: | ---: | ---: | ---: |
| Short email | 20 | 0.273 | 0.283 | +3.7% | 0.762 |
| Mixed, all seven | 166 | 1.740 | 1.694 | -2.6% | 4.545 |
| Long mixed (100 repeats) | 16,600 | 172.428 | 171.909 | -0.3% | 380.441 |
| Adversarial near-matches/digits/context | 5,300 | 30.278 | 30.594 | +1.0% | 52.054 |

No measured no-locale regression exceeded 10%. German scans intentionally do
additional matching and allocate additional findings. Seven fixed regexes are
compiled once, use fixed-width values and horizontal context gaps, and operate
on borrowed input without per-candidate whole-document copies. No-locale scans
do not initialize the German patterns.

Reproduce each binary with `cargo run --release -p datafog-core --example
german_benchmark -- 1000`. Compare repeated release runs on the same machine,
using the same example source in both revisions.

## Migration and release boundary

See `guides/migrating-from-datafog-python.mdx` for the separate 4.8.1 migration
matrix: ASCII digits, four horizontal separators, immediate bounded context,
Core locale routing instead of scan entity selection, retained overlaps and
legacy document heuristics. IBAN checksum-invalid candidates remain detected.
No package versions or legacy Python fixtures were changed, and no packages
were published. A compatible published wheel and subsequent adapter pin update
remain necessary for Python 4.9 adoption.
