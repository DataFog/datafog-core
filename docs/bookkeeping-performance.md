# Slice Eleven: offset calculation and record conversion

This is a focused follow-up to [Slice Ten](finding-selection-performance.md).
The baseline is the merge of PR #13, commit
`083eaaab6bcfcfba9a31bc8ce1cfc99f1c2a3cae`. The change is a candidate for the
planned 0.3.0 release; package versions stay at 0.2.0 until release preparation.

## Scope and behavior

Dense inputs still performed repeated prefix walks after overlap selection was
optimized. Core converted each finding's code-point positions independently,
recounted non-ASCII prefixes when producing scan findings, and recounted the
growing output when producing transformation records. JavaScript bindings also
recounted prefixes for UTF-16 ranges. Node structured result conversion copied
the entire output field for each transformation or restoration record.

This change:

- Adds Core's reusable `TextIndex`, which borrows one immutable string and
  calculates byte, code-point, and UTF-16 positions using lazy checkpoints.
- Reuses indexes during Core finding validation and scan finalization, and
  during Node/WASM conversion of findings and transformation records. Node
  restoration records use the same conversion helper. Structured indexes are
  scoped to individual fields, including when supplied findings are unordered.
- Maintains a running output code-point count while applying transformations.
- Converts individual Node structured records directly against borrowed field
  text, eliminating the per-record copy of that text.

Existing operation signatures, configurations, output shapes, detector coverage,
selection policies, error precedence, finding indices, and provider sequencing
are unchanged. `TextIndex::new` and `TextIndex::utf16_range` are additive Rust
helpers shared by the JavaScript bindings; the existing scalar `utf16_range`
function remains available. No dependencies, model files, or dictionaries are
added. Indexes are local to an operation and are not cached across requests.

## Index cost

An index advances through newly requested text once, storing one checkpoint per
256 code points. A lookup behind the furthest visited position searches the
checkpoints and walks at most 256 code points. For `C` visited code points and
`m` lookups, this costs `O(C + m(log(C/256 + 1) + 256))`, with `O(C/256)` stored
checkpoints. Ordered forward lookups take `O(C + m)` work. This replaces repeated
prefix walks that could cost `O(Cm)`.

Each checkpoint stores three `usize` offsets (24 bytes on a 64-bit target), plus
the vector's spare capacity. Strings shorter than 256 code points need no
checkpoint allocation, and text beyond the furthest requested position is not
indexed. Running output counts visit newly appended text instead of revisiting
the entire output. This is an algorithmic accounting, not a process-level memory
benchmark or a bound on every part of a complete request.

## Reproduce the comparison

Use Node 24 and the same Rust toolchain for both builds. From the candidate
repository, create a separate baseline checkout and build both native packages:

```sh
git worktree add --detach ../datafog-core-bookkeeping-baseline 083eaaab6bcfcfba9a31bc8ce1cfc99f1c2a3cae
npm ci --prefix ../datafog-core-bookkeeping-baseline/bindings/node
npm run build --prefix ../datafog-core-bookkeeping-baseline/bindings/node
npm ci --prefix bindings/node
npm run build --prefix bindings/node
node scripts/benchmark-bookkeeping.mjs ../datafog-core-bookkeeping-baseline/bindings/node/index.js bindings/node/index.js target/bookkeeping-results.json
```

The [benchmark](../scripts/benchmark-bookkeeping.mjs) loads both native packages
in one process and compares complete results before timing. It measures scan,
transformation of supplied findings, and combined scan-and-transform separately.
Both implementations are warmed and measured in alternating order over seven
rounds; the JSON report contains every sample and its median. Timings include
JavaScript input checks, JSON transport, Core work, and result conversion. They
exclude package import and provider I/O. Baseline findings are supplied to both
standalone transformations. The policy redacts by default and masks emails.

## Local results

Recorded 2026-09-04 on macOS ARM64, Rust 1.88.0, Node 24.19.0. The baseline native
package was copied from the unchanged baseline build before editing Core. The
table shows median **combined scan-and-transform** time, in milliseconds:

| Workload | Findings | Baseline | Candidate | Speedup |
| --- | ---: | ---: | ---: | ---: |
| One customer record | 3 | 0.0179 | 0.0180 | 0.99× |
| 100 customer records | 300 | 1.430 | 1.454 | 0.98× |
| Long Unicode field, one email | 1 | 2.555 | 1.345 | 1.90× |
| Dense Unicode field | 128 | 4.109 | 0.393 | 10.45× |
| Dense Unicode field | 512 | 61.189 | 1.564 | 39.12× |
| Dense Unicode field | 1,024 | 242.311 | 3.204 | 75.63× |
| Dense ASCII field | 1,024 | 137.840 | 3.265 | 42.22× |

Doubling dense Unicode findings from 512 to 1,024 took 3.96× as long on the
baseline and 2.05× on the candidate. Standalone transformation of the 1,024
Unicode findings improved from 243.682 ms to 5.453 ms (44.69×); scanning improved
from 8.987 ms to 1.291 ms (6.96×).

There are tradeoffs. Across the small/customer workloads, the candidate was
approximately 0–4% slower depending on the operation. Scanning the long sparse
Unicode field was 0.837 ms versus 0.762 ms (about 10% slower), although its
combined operation improved. These shared-machine measurements are not latency
guarantees and do not establish that every workload improves. The measured
benefit is strongest when a field contains many findings.

## Verification and remaining work

- Rust formatting, Clippy with warnings denied, and all workspace tests pass:
  84 tests; the Slice Ten manual selection benchmark remains ignored by default.
- Index tests compare ascending and descending lookups with direct Unicode
  counts across checkpoint boundaries, including emoji, combining characters,
  ASCII, empty strings, malformed byte boundaries, and oversized offsets.
- Cached validation retains the first error and original finding index even
  after looking up later source positions and when entity filtering excludes
  the findings.
- Installed Python, Node, and browser WASM conformance tests pass. Additional
  Node/WASM tests verify dense multi-field byte/code-point/UTF-16 output ranges,
  reversed findings, redaction, removal, and emoji masking. Node also verifies
  a dense structured tokenization/restoration round trip.
- Every benchmark workload returns identical complete findings and transformed
  results on the baseline and candidate.

Validation and selection can still repeat across preparation/completion stages.
JSON transport, structured token routing, Core restoration prefix counts, and
the mixed-confidence overlap fallback are not redesigned here. The next pass
should profile representative remaining costs before introducing reusable
prepared-request state. This change does not claim an end-to-end complexity
bound for arbitrary requests or a complete removal of internal bookkeeping.
