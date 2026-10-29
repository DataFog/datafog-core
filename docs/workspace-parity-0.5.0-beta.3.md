# Native workspace parity (active draft)

[#171](https://github.com/DataFog/datafog-core/issues/171) compares deterministic
Core workflows across Rust, actual installed native Python/Node and browser WASM.
`fixtures/workspace/sdk-contract-v1.json` specifies namespaces, operation IDs,
source/configuration, mutation order and expected errors. Canonical INNER snapshot
bytes, markers, full lossless heads, all source/output ranges, metadata, policy,
recall/review/restore reports and failures must match. Host ciphertext, nonces and
provider engine accuracy are outside this exact contract comparison.

The current profile covers the committed launch workspace surface. Missing
mandatory bindings/methods, failed or malformed calls, unexpected codes and any
mismatch are failures; declarations/mocks/skips do not qualify. A fixture's
`expectCode` permits only that documented operation failure, not a missing method
or harness failure. Result keys normalize only representation (Python's exact
revision integer to its decimal wire string, copied bytes to lowercase hex),
without discarding offsets, choices, flags or errors.

Streaming, operation/conversation provenance and actual W6 CAS/retry cases remain
required additions before X8 can close. This foundation profile must never be
presented as full 0.5.0-beta.3 or real-data security certification. The actual
native wrappers are draft PRs #174 and #175; this branch integrates their committed
code solely for qualification, not into the beta.2 release branch.

## Run and inspect

After `cargo dfv build` and `cargo dfv doctor`, run:

```sh
cargo dfv parity --workspace --json
```

This executes all four SDKs with no sampling or single-SDK bypass. The blocking
verification workflow runs it after the normal scan parity/smoke gates. Protocol
failure tests reject missing methods, malformed/incomplete results and a restored
prefix reported as success alongside marker issues. A dry run explicitly reports
`pass: false` and `executed: false`, even though the command itself can exit normally.

Reports live under `target/dfv/runs/<timestamp>/workspace-parity/`: raw canonical
per-SDK outputs, the exact fixture, Python wheel/Node/WASM tarballs, SHA-256 receipts,
runtime versions, source commit/dirty state, doctor data and final pass/failure.
Python selection requires exactly one wheel matching the current package/version
in `bindings/python/pyproject.toml`, then checks the installed distribution version.
Historical wheels remain retained; they cannot replace the current wheel or make
an otherwise valid current build ambiguous. Multiple current candidates or a
missing/invalid pin fail rather than selecting an arbitrary artifact.
The CLI never deletes reports. Temporary installed consumers and browser profiles
are cleaned separately. Any process/runtime/parse failure prevents a complete
comparison and returns nonzero without retaining arbitrary child diagnostics.

The current matrix has 17 scenarios and 156 real operation steps, including every
current workspace error code, Unicode/source metadata, correction conflicts,
failed activation receipts, protected policy, repeated operation IDs, atomic batch
failure, strict marker reports, canonical reopen/recovery, v1→v2 upgrade and a
revision above JavaScript's exact numeric range. Only representation defaults
normalize: absent nullable finding metadata becomes null and revision integers
become lossless decimal strings. Matched text, offsets, flags and choices are kept.

The matrix includes all 11 shared review-conflict scenarios: source flags cannot
spoof a channel or native SSN precedence, duplicate origins retain metadata,
containing coverage remains safe, crossing ranges stop handoff, exact not-sensitive
choices remove only the target obligation, and malformed waived findings still
reject before filtering. Additional committed-recall scenarios check Unicode
case/whitespace expansion, Ann versus Annette, exclusion of unapproved fuzzy
matches, no recall before activation, and every overlapping committed identity.
Crossing recall candidates remain visible and prevent preparation across all SDKs.

The reference adapter validates plan access before explicit `discard`, matching
the public opaque SDK lifecycle contract. Core's lower-level cleanup remains
idempotent; implicit/context cleanup is not treated as a second explicit public
operation. Repeated explicit discard and discard after owner close are exercised
in the same fixture across all SDKs.
