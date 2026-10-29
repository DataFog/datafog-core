# Native Node workspace contract — 0.5.0-beta.3 candidate

Implementation work for [X7 / #168](https://github.com/DataFog/datafog-core/issues/168),
integrating released beta.2 main and the approved Core streaming candidate #176. This is an in-process
Rust N-API binding. It does not load browser WASM or implement a persistence host.
The reviewed lifecycle and streaming calls are exposed. The current draft also
wraps shared Core [operation records/counts and pages](native/node-operations.md)
for #147. The current Node candidate also wraps the committed Core conversation
methods for #148, with the same explicit saved-send and scoped-restore contract.
The package is not released by this draft.

## Calls, ownership and representations

| Public Node call | Shared Core call | Ownership / representation |
|---|---|---|
| `Workspace.create`, `openBundle` | `Workspace::create`, `open_bundle` | New owned native workspace; 16-byte namespace/operation IDs, bounded snapshot copies |
| `head`, `namespace`, `exportBundle` | `head`, `namespace`, `export_bundle` | Independent output copies; revision is a canonical decimal string, never a JS number |
| `prepare`, `prepareBatch`, `prepareReview` | corresponding Core prepare methods | Opaque plan belongs to exactly one workspace instance; no committed mutation |
| `review`, `recall`, `policy` | corresponding read-only Core methods | Complete original byte/codepoint/UTF-16 coordinates; no allocation of aliases or host approval |
| `preparePolicyChanges`, `prepareRecovery` | Core staged metadata/recovery methods | Exact approved policy and warned v1/v2-to-v3 draft upgrades; no handoff permission |
| Plan `preview(s)`, `heads`, `policyPreview`, `snapshotFormat`, `exportCandidateBundle` | Core prepared-change getters | Sensitive local reads, copied output; export is plaintext for host encryption |
| `prepareConversationSend`, `prepareObservedSend`, `prepareDefinitelyNotSent` | Core staged send preparation/resolution | Opaque normal save/activate plans; Core reserves later confirmation capacity and derives intended markers |
| Plan `sendIntents`; `conversation`, `conversationForSite`, `sendIntentStatus` | Core saved context/intent views | Owned metadata copies; intent IDs alone grant no restoration or transport authority |
| `restoreInConversation`, `restoreReportInConversation`, scoped buffered/progressive factories | Core conversation-aware restoration | Exact logical identity; unresolved intent pauses every scoped response in that conversation |
| Plan `discard` | `PreparedChange::discard` | Consume pending plan; release its native storage; garbage collection also releases orphaned plan slots |
| `activateSaved` | `Workspace::activate_saved` | Host first confirms durable complete-head CAS; rejects cross-instance/stale/consumed plans |
| `restore`, `restoreReport` | corresponding Core restore methods | Whole-response fail-closed restoration |
| `bufferedRestore`, `progressiveRestore` | Core owned restoration adapters | Factory-only child pinned to the committed head; strict buffering or explicit irreversible chunks |
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
not prove execution. Future Core provenance/conversation additions must
extend this matrix and all SDKs before [G3 / #154](https://github.com/DataFog/datafog-core/issues/154)
freezes the launch contract.

## Restoring response chunks

`bufferedRestore("utf8" | "utf16")` retains the strict R2 behavior: `feed(Uint8Array)`
and `feedUtf16(Uint16Array)` release no text, and `finalize()` returns the complete
restoration report. `progressiveRestore()` is an explicit opt-in to irreversible
output: successful feeds return `{text, delivered, complete: false}`; only a
successful finalization sets `complete: true`. Provider framing is host-owned.

Both adapters call the same Core implementation as WASM, including all marker,
UTF-8/UTF-16 and cumulative resource limits. A terminal progressive error preserves
prior delivered counts and original issue ranges, has `complete: false`, and
contains no failed-call text. Previously returned chunks cannot be retracted.
Use `reset()` only for a fresh response at the same current head; it does not undo
output. Close, owner garbage collection and saved head changes release private
child state; a same-head no-op save preserves children. Transport accepts only the
specified typed arrays and copies them without changing the caller's buffers.

Installed tests execute the shared streaming fixture, every byte/code-unit split,
malformed transport and terminal errors, plus native worker, garbage-collection,
same-head and size-limit lifecycle cases. These are candidate beta.3 APIs and are
not present in the published beta.2 packages.

## Conversation binding evidence and limits

The public conversation shapes and safe plain-property validation match WASM.
Malformed context-definition and site-lookup fields use `invalid_conversation`;
malformed intents use `invalid_send_intent`, direct logical-ID selectors retain
`invalid_operation_id`, and source text retains `invalid_finding`.
Context and intent IDs accept exact 16-byte `Uint8Array` or dense unsigned-byte
arrays for owned-view round trips; direct logical-ID parameters require
`Uint8Array`. No accessor executes during shape validation. Per-member errors
retain `inputIndex`; Core owns interpretation, capacity, membership, one-time
binding and canonical decoding. All ten workspace methods and plan `sendIntents`
use the existing native plan/child registry. New error codes and scoped marker
issues preserve the complete existing error fields.

`node scripts/test-node-package.mjs --prebuilt` packs and installs the actual
native package, executes the shared 71-check conversation corpus, then adds
Node transport/ownership, 32-plan, scoped UTF-16 and child-lifetime tests. It also
uses the existing encrypted SQLite store for a failed first save, durable pending
reopen, lost confirmation acknowledgment and replay rejection, plus an independent
worker reopening and streaming the confirmed state. These are synthetic trusted
observations; no actual provider send, extension identity acquisition, production
host approval or four-SDK conversation parity is claimed. Existing foundation,
streaming, operation and host checks remain mandatory.

Saving a conversation transaction uses the coordinated unreleased v3/key9 format.
Released v1/v2 state preserves exact read/no-op bytes until a real saved upgrade;
warn before upgrading because published beta.1/beta.2 cannot reopen v3. A pending
intent is not successful issuance; only the second durably saved trusted observed
send enables its context's issued membership. An uncertain save requires reloading
current saved state, with no resend or implicit unscoped fallback.
