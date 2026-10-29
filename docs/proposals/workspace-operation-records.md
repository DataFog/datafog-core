# C1/C3 operation records and counts: implementation proposal

**Draft for review, targeting `0.5.0-beta.3`.** This implements a concrete proposal
for [#147](https://github.com/DataFog/datafog-core/issues/147), based on the
[C1/C3 contract draft in #158](https://github.com/DataFog/datafog-core/pull/158).
The API, retention and snapshot choices are not approved or frozen. This work
does not publish a package, complete conversation handoffs or settle snapshot
compatibility for the launch.

## Recommended choices for review

1. Retain up to **10,000** operation records without automatic deletion or
   compaction. Reaching that limit, or the existing 16 MiB bundle limit, fails
   explicitly with `resource_limit`. Raising or changing retention needs review;
   old operation IDs are never silently forgotten.
2. Use a fresh host-generated random 16-byte ID for every committed operation.
   IDs are workspace-local replay checks, not credentials or proof of external
   delivery. Use the normal expected-parent/candidate-head save contract.
3. Treat host `occurredAtMs` as untrusted UTC Unix milliseconds in
   `0..253402300799999`, including both endpoints. It does not order commits or
   authorize activity; the head revision orders commits. Nonmonotonic host times
   are accepted. Legacy mutations have `null` time, not a fabricated current time.
4. Add bounded numeric `preparedInputs` and `transformedSpans` deltas to each
   operation record. These differ from #158's three-field record proposal and
   allow the decoder to reconstruct and verify persisted totals without raw text.
5. Keep legacy no-change `prepare`/`prepareBatch` byte- and head-preserving. An
   explicitly recorded no-change preparation uses the new timed call. This is a
   deliberate compatibility choice requiring review against #158, which suggests
   recording successful preparation input members even with empty findings.

## Proposed API and transaction behavior

Rust provides `OperationContext`, `OperationOutcome`, `OperationRecord` and
`OperationCounts`; WASM exposes matching `WorkspaceOperation`, `WorkspaceOutcome`,
`WorkspaceOperationRecord` and `WorkspaceCounts` declarations.

```ts
const operation = {id: crypto.getRandomValues(new Uint8Array(16)), occurredAtMs: Date.now()};
const change = workspace.prepareBatchWithOperation(inputs, operation);
const {expectedHead, candidateHead} = change.heads();
const encrypted = await encryptForHost(change.exportCandidateBundle());
await saveAtomically(encrypted, expectedHead, candidateHead);
workspace.activateSaved(change, expectedHead, candidateHead);
const records = workspace.operations();
const counts = workspace.counts();
```

The host supplies encryption, approval and durable saving. Core cannot verify a
host's durability assertion. Candidate export can be repeated for an unsaved
retry; candidate contents and committed records/counts remain unchanged until
activation. A rejected save, stale parent/candidate, discard, lock/close, preview,
getter or restore call adds no committed record. Saving then losing acknowledgment
requires reopening the saved bundle before another attempt; an operation ID does
not provide exactly-once site delivery.

The added WASM entry points are `createWithOperation`, `prepareWithOperation`,
`prepareBatchWithOperation`, `prepareRecoveryWithOperation`, `prepareOutcome`,
`operations` and `counts`; Rust uses the corresponding snake-case names. Timed
preparation explicitly stages one operation and advances the head even with no
new mappings or findings. Empty batches remain invalid; a single empty text input
is a valid explicit preparation input. A batch records one operation, its input
member count and the number of replaced occurrences, including repeated values.

Creation bootstraps an initial record at revision zero; that initial state is
unsaved until the host persists it. Creation is the only API that starts a new
workspace without activating a preexisting candidate. Existing create/real
mutation calls also write records, with unknown (`null`) time. A legacy
preparation that introduces no mappings retains its original no-op behavior.
Read-only export of any opened bundle does not upgrade it.

The runtime rejects IDs present in retained records. New explicit calls return
`operation_reused` for all recognized replays. Legacy calls preserve their
existing `invalid_operation_id` for current-head or mapping-issuance IDs and use
`operation_reused` for other retained historical IDs. This error compatibility
choice is another explicit difference from #158. An imported legacy snapshot
cannot reveal operations whose IDs were never stored; only its current head and
mapping issuance IDs remain checkable. No lost history is invented.

## Content-free records and count definitions

Each record contains only random operation ID, bounded kind, nullable timestamp,
input count and transformed-occurrence count. There is no source text, identity
key, mapping value, URL, arbitrary error message or free-form context. The two
numeric deltas are positive/bounded only for preparation records; every other
kind has zero deltas.

| Count | Meaning |
| --- | --- |
| `operations` | Number of retained committed operation records, including create/recovery and explicitly recorded outcomes. |
| `preparedInputs` | Sum of input members in recorded preparation operations. Legacy no-change preparations are unrecorded. |
| `transformedSpans` | Replaced source occurrences in recorded preparation operations, not distinct identities or marker references. |
| `confirmedHandoffs` | Zero in this slice. Only a later reviewed observed-insert/send transaction may increment it. Copy confirmation is not a handoff. |
| `reportedBlockedAttempts` | Number of explicitly committed `blocked_input` or `restore_blocked` outcome records, not the number of underlying issues. |

`prepareOutcome({kind, issueCount}, operation)` accepts `copy_confirmed`,
`user_cancelled`, `blocked_input` and `restore_blocked`. Blocked outcomes require
1–10,000 issues; copy/cancel require zero. The issue count validates the bounded
assertion and is not retained. No event is automatically recorded on a failure,
cancellation, copy callback or blocked restore. A host deliberately stages and
saves the optional outcome after observing it. Omitting that record does not
convert the original failure into success.

These are totals over the retained documented history, not lifetime telemetry.
Policy-change records and observed conversation handoffs are not implemented;
their later APIs must use the same staging/count invariants.

## Snapshot and backup compatibility proposal

The outer encrypted vault/backup formats are unchanged by this branch. The inner
workspace CBOR snapshot gains **version 2**:

- Keys 1–6 preserve the existing namespace, head, entries, issuance counters and
  synthetic privacy configuration. Key 0 contains version 2.
- Key 8 is a bounded array of operation tuples
  `[id16, kindCode, occurredAtMsOrNull, preparedInputs, transformedSpans]`.
- Key 10 contains the five aggregate counts in the public field order above.
  Key 9 remains absent/reserved for a later reviewed conversation proposal.
- The decoder recomputes every total from operations and requires exact equality.
  It rejects duplicate IDs, unknown kinds/keys, malformed/noncanonical/duplicate
  CBOR, invalid times, inconsistent heads, invalid numeric deltas and over-limit
  histories before admitting state. New workspaces retain complete create-based
  revision history; upgraded legacy snapshots may start after unknown history.

Version-1 snapshots remain readable and re-export **byte for byte** with empty
records and zero covered-history counts. No migration occurs on open/read/export,
no-op legacy preparation or discarded candidates. The first explicit saved
upgrade/mutation stages version 2 through prepare/export/save/activate. That
candidate preserves mappings, marker bytes, counters and privacy configuration,
then adds only the new operation. Recovery similarly preserves old markers and
adds its own new operation; it does not invent prior history.

An old decoder cannot open version 2. This is a proposed compatibility boundary,
not a frozen migration policy or a promise that beta.1 can open future backups.
The snapshot freeze, policy/source fields, conversation fields and final backup
round-trip matrix must be reconciled before merging dependent implementations.

## Evidence and remaining review

Native tests cover exact batch/occurrence counts, no-change behavior, timestamp
limits, replay/stale-head rejection, read/discard/failure preservation, canonical
and malformed snapshot imports, retention exhaustion without ID eviction, and
byte-preserving legacy import followed by a staged upgrade.

The packed SDK tests exercise matching WASM APIs and TypeScript declarations,
including malformed contexts, accessor rejection, invalid outcomes and mutable
copies returned by getters. The actual MV3 extension test uses Web Crypto and
real IndexedDB to abort and retry a timed batch, checks no count change on
discard, reopens encrypted records, exports a backup, approves restore into an
empty profile, restarts the browser and verifies preserved counts plus one
legacy-timed recovery operation. Existing backup password/recovery paths and
rollback/cancel/race checks run against the new snapshot keys too.

Before this can merge, review the five choices above, the legacy reuse-error
split, old-decoder incompatibility, and alignment with #143/#158 and forthcoming
policy/handoff transactions. A successful local suite proves these draft
behaviors, not approval of those product decisions or completion of C1/C3.
