# Native Node workspace contract — 0.5.0-beta.3 candidate

Implementation work for [X7 / #168](https://github.com/DataFog/datafog-core/issues/168),
stacked on the committed combined-review candidate #173. This is an in-process
Rust N-API binding. It does not load browser WASM or implement a persistence host.
Only methods present in the committed shared Core are exposed; provenance,
conversation and streaming methods join once their compatible Core contracts land.
The package is not released by this draft.

## Calls, ownership and representations

| Public Node call | Shared Core call | Ownership / representation |
|---|---|---|
| `Workspace.create`, `openBundle` | `Workspace::create`, `open_bundle` | New owned native workspace; 16-byte namespace/operation IDs, bounded snapshot copies |
| `head`, `namespace`, `exportBundle` | `head`, `namespace`, `export_bundle` | Independent output copies; revision is a canonical decimal string, never a JS number |
| `prepare`, `prepareBatch`, `prepareReview` | corresponding Core prepare methods | Opaque plan belongs to exactly one workspace instance; no committed mutation |
| `review`, `recall`, `policy` | corresponding read-only Core methods | Complete original byte/codepoint/UTF-16 coordinates; no allocation of aliases or host approval |
| `preparePolicyChanges`, `prepareRecovery` | Core staged metadata/recovery methods | Exact approved policy and v1/v2 upgrade behavior; no handoff permission |
| Plan `preview(s)`, `heads`, `policyPreview`, `snapshotFormat`, `exportCandidateBundle` | Core prepared-change getters | Sensitive local reads, copied output; export is plaintext for host encryption |
| Plan `discard` | `PreparedChange::discard` | Consume pending plan; release its native storage; garbage collection also releases orphaned plan slots |
| `activateSaved` | `Workspace::activate_saved` | Host first confirms durable complete-head CAS; rejects cross-instance/stale/consumed plans |
| `restore`, `restoreReport` | corresponding Core restore methods | Whole-response fail-closed restoration; no streaming claim |
| `close` | `Workspace::close` | Idempotent; clears retained plans and invalidates all outstanding capabilities |
| `workspaceByteRangeFromUtf16` | `byte_range_from_utf16` | Validates original scalar boundaries; no replacement of unpaired surrogates |

All calls are synchronous and bounded by Core resource limits. They execute on
the calling Node isolate. The wrapper grants no cross-thread shared-memory handle:
worker messages must carry authenticated saved data, not a live workspace/plan.
In-process handles and their 32-live-plan bound are not distributed locks or
host-wide admission control. Worker ownership, actual atomic storage races and
stateless mapping-state requirements are qualified separately by
[W6 / #170](https://github.com/DataFog/datafog-core/issues/170).

The JS layer validates transport shapes, plain data properties, well-formed
strings, buffers, safe offsets and bounded JSON. Rust Core owns identity,
protected-type choices, overlap coverage, offsets, marker parsing, canonical
snapshots and transaction semantics. No detector or privacy policy is implemented
in JavaScript. Native outputs are not writable aliases of Core secret storage.

## Errors and host duties

Workspace errors preserve their Core `code`, content-free `findingIndex` and
`inputIndex`, ranged `markerIssues` and `reviewConflicts`. Missing/invalid methods,
closed or consumed handles, malformed input and resource exhaustion never become
empty success. Existing detector/model/provider errors retain their old fields.
Heads use the full revision and commit ID; input accessors and lossy uint64 numbers
must reject before native execution.

The host supplies CSPRNG IDs, user review/approval, encryption, key storage,
durable save-if-head-matches, request/tenant ownership and output release. A save
failure releases no text. An uncertain acknowledgment requires reopening durable
state and fresh review. Explain before a saved v1-to-v2 upgrade that beta.1 cannot
open the upgraded vault. Credentials, encryption and durable state never move
into this binding.

## Proof required before shipping

Pack/install the native npm package and compile real TypeScript consumers. Execute
lifecycle, ownership, batches, policy/choice, recall/review/correction, original
Unicode offsets, strict malformed-snapshot/recovery and uint64-head cases against
the installed native library. Exercise consumed/stale/closed plans and worker
isolation. Shared [X8 / #171](https://github.com/DataFog/datafog-core/issues/171)
fixtures compare actual installed native output with Rust/browser/Python and fail
on missing, skipped, unsupported or malformed calls. Declaration tests alone do
not prove execution. Future Core provenance/conversation/streaming additions must
extend this matrix and all SDKs before [G3 / #154](https://github.com/DataFog/datafog-core/issues/154)
freezes the launch contract.
