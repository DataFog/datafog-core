# C1/C3 operation records and counts — v3 review proposal

Draft for #147 / `0.5.0-beta.3`, reconciling #143/#158 with the committed
policy-v2 implementation. **Do not merge or publish this draft yet.** Sid approved
preserving every operation record: no deletion, sampling or summary replacement.
Lossless compression is separate postlaunch work in #165. Sid also approved
recorded-history-only counts, explicit unknown pre-upgrade history and per-record
numeric input/replacement deltas on October 29. He then approved the provisional
100,000-row ceiling with the final 256 slots reserved for recovery. Legacy replay
errors and the remaining accounting details below are still unapproved.

## Compatibility and transaction boundary

Policy-v2 already uses snapshot version 2 and keys 6/7. This draft therefore uses
**version 3**, never the old unmerged #162 version-2 layout. The old draft layout
was not released and is rejected rather than being mistaken for a policy vault.

The proposed v3 map retains keys 1–7 and their current meaning, adds operation
rows at key 8 and covered-history counts at key 10. Key 9 remains reserved for
#148 conversations. This is one coordinated beta.3 format proposal, not permission
for a separate conversation implementation to claim a conflicting version 3.
Conversations and their accounting must be reconciled before the format freezes.

Actual released v1 and policy-v2 bundles open and re-export byte for byte with
empty covered history. Reads, legacy no-change preparation, no-change policy
edits, discarded candidates and failed activation preserve their original bytes,
head and version. A real saved mutation or explicit recorded operation stages v3
through prepare → candidate export → approved durable save → activation. No lost
history or historical timestamp is invented. New draft workspaces bootstrap a v3
create record; bootstrap is unsaved until the host actually persists it.

**Before approving a v1/v2-to-v3 save, explain that `0.5.0-beta.1` and
`0.5.0-beta.2` cannot open the upgraded vault.** The reference host shows the notice
before text/settings saves and legacy backup recovery. Existing backup files are
unchanged. The stronger password-reauthenticated 256 MiB backup envelope and both
password/recovery imports are preserved; inner-format changes do not restore the
old weaker password wrapper.

## Concrete choices still needing review

### Capacity and recovery

The code retains the original **10,000-record draft limit only as a candidate**.
It is not the selected launch limit. Reaching it fails without dropping IDs or
committed data. This combination has an important defect: recovery appends a
record too, so a backup already at the record limit cannot complete normal
fresh-head import. The regression test makes that failure explicit; it is not an
approved shipping behavior. Read/export still work.

**Approved October 29:** a provisional **100,000-record hard ceiling**, with the final 256
slots reserved for recovery and a small encoded-byte reserve for recovery
metadata. Ordinary recorded activity would stop at 99,744 total rows; a fresh
recovery could still retain every row and append its own record. This is a
bounded beta proposal, not unlimited history or an indefinite-recovery promise.
At the hard record/byte ceiling, writes including further recovery still fail;
read/export must remain available and the host must make the limit visible.
The byte reserve must be derived from the final #148 wire schema, not guessed.
The count ceiling and 256-slot recovery reserve are approved; this draft
still needs their implementation and boundary/browser/recovery qualification.
The encoded-byte reserve must match the supported final wire format.

Why this recommendation: 10,000 rows are inexpensive but can be exhausted after
5,000 round trips if an application records preparation and copy confirmation.
At an illustrative 60 round trips/minute that is about 83 minutes. That is a
scenario, not measured user traffic. The 100,000-row experiment costs materially
less memory than 300,000 rows while leaving room in the existing 16 MiB snapshot
budget. Persisting all history in one finite snapshot still has a lifetime limit;
compression or a later reviewed archive/segmentation design cannot be silently
substituted for retaining every record during these betas.

### Accounting

**Approved October 29:** retain per-record numeric `preparedInputs` and
`transformedSpans` deltas so the decoder can independently recompute the totals.
These extend #158's three-field proposal. Counts cover only actually recorded
actions; older vaults explicitly have unknown pre-upgrade history, not invented
lifetime totals. No source value, identity key, URL, arbitrary message or free-form
context enters the row. This approval does not choose the specific action/kind,
timestamp, replay, capacity or recovery behaviors discussed below.

The candidate keeps legacy no-op bytes and uses explicit timed calls to record a
successful no-change preparation. Real legacy preparation/recovery mutations
record unknown (`null`) time. A batch records one operation, its input-member
count and replaced occurrences, including repeated values. Current policy saves
also append a proposed `policy_change` row with zero input/span deltas and unknown
time; combined review delegates to the same preparation path. These accounting
choices remain proposals. A policy no-op stays byte-preserving.

Counts describe **recorded covered history**, not all lifetime activity. Older
vaults have unknown prior history, and successful legacy no-change preparations
remain unrecorded. The API/UI must communicate that boundary before describing
these numbers as lifetime totals. Host UTC Unix milliseconds are untrusted
assertions, accepted in `0..253402300799999`; head revision, not host clock,
orders committed mutations. Nonmonotonic times are allowed.

`confirmedHandoffs` stays zero. Copy confirmation is not proof of site insertion,
send, or provider delivery. #148 must implement a reviewed observed-handoff
transaction before that counter changes.

### Reused IDs

Recommended: legacy APIs consistently retain `invalid_operation_id` for recognized
reuses, while new explicit operation APIs return `operation_reused`. The existing
candidate still has the older proposed split: legacy current-head/mapping/settings
IDs return `invalid_operation_id`, but other retained historical IDs return
`operation_reused`. That difference is not approved. Existing records are never
silently forgotten. Older snapshots cannot reveal removed/unrecorded IDs; the
runtime cannot reconstruct them or promise to detect unknowable replays.

## Proposed API and count definitions

Existing proposed Rust calls are `create_with_operation`, `prepare_with_operation`,
`prepare_batch_with_operation`, `prepare_recovery_with_operation`, `prepare_outcome`,
`operations` and `counts`. WASM exposes matching camelCase calls. Native Python,
Node and X8 must extend the complete actual-call/error matrix after this contract
is approved; declarations or placeholders do not count as implementation.

An operation context contains a fresh host-generated random 16-byte ID and bounded
`occurredAtMs`. IDs are workspace-local replay checks, not credentials. A pending
operation follows the normal complete-parent/candidate-head save contract. Preview,
read, restore, candidate export, discard, failed save and stale activation do not
change committed records or totals. Losing an acknowledgment requires reopening
saved state and fresh approval; an ID does not prove exactly-once external delivery.

| Counter | Proposed covered-history meaning |
| --- | --- |
| `operations` | Every retained committed row, including create/recovery and explicit outcomes. |
| `preparedInputs` | Sum of recorded preparation input members; not unrecorded legacy no-ops. |
| `transformedSpans` | Replaced source occurrences, not distinct identities or marker references. |
| `confirmedHandoffs` | Zero until #148's observed-handoff transaction exists. |
| `reportedBlockedAttempts` | Explicitly saved blocked-input/restore outcomes, not underlying issue count. |

Rows contain ID, bounded kind, nullable time and the two numeric deltas. Only
preparation rows have nonzero deltas. `prepareOutcome` supports `copy_confirmed`,
`user_cancelled`, `blocked_input` and `restore_blocked`. A blocked outcome validates
1–10,000 issues but does not retain their content/count; copy/cancel require zero.
No callback or failure automatically records an outcome. Internal consistency of
these rows does not attest that a reported external action actually happened.

The strict decoder checks canonical/duplicate/truncated CBOR, exact version/map
shape, kinds, IDs, times, deltas, complete heads, resource bounds and recomputed
counts. Full create-based history must account for its revisions. Upgraded
v1/v2 history starts after an unknown prefix. Mappings/settings retain their
existing first spelling, aliases, counters and protection rules.

## Measured capacity experiments

Synthetic optimized native Rust runs on this Apple arm64 machine, with no model
or provider call. These are single-run observations, not stable-runner benchmark
gates, lower-hardware guarantees, Chrome timings or selected capacity defaults.
The 100k/300k limits were temporary measurement builds and were restored to the
original unapproved 10k candidate afterward.

| Experimental ceiling | Rows tested below full | Wire bytes | Core open | Timed preparation | Record backing after append |
| --- | ---: | ---: | ---: | ---: | ---: |
| 10,000 | 9,999 | 330,054 | 0.98 ms | 0.59 ms | 560,000 B |
| 100,000 | 99,999 | 3,300,060 | 11.40 ms | 6.93 ms | 5,600,000 B |
| 300,000 | 299,999 | 9,900,060 | 35.21 ms | 21.04 ms | 16,800,000 B |

The profiles use maximum numeric deltas and fixed synthetic timestamps. The
record struct is 56 bytes on this target. Clone-and-append initially doubled its
vector capacity; `try_reserve_exact(1)` now avoids that unnecessary doubling and
surfaces allocation failure. These backing figures exclude allocator overhead,
ID-validation trees, existing mappings/settings, encoded bundle capacity and
runtime/binding copies. With 32 pending candidates, records plus wire payload
alone are roughly 285 MB at 100k rows and 854 MB at 300k. Host admission limits
remain necessary; there is no global Core counter across workers.

Reproduce in an idle isolated worktree with `python3 scripts/measure-workspace-operations.py`.
It runs the checked-in `workspace_operation_cost` example, restores the original
proposal limit in `finally`, and rebuilds the ordinary default-feature example.
Rebuild SDKs afterward before reusing their binaries. [Checked-in experimental receipts](evidence/workspace-operation-capacity/manifest.json)
include source hashes, the explicitly dirty integration baseline and tested cap; only synthetic fixtures are constructed. Rows above an experiment's admission
limit report rejection separately; their small rejection time is not a successful
large-history decode measurement.

## Verification and remaining gates

Content-free metadata is still private vault data, not public telemetry, and is
never automatically logged. The reconciliation adds an actual native-policy-v2 export fixture and tests exact
v1/v2 read/no-op/discard/failed-save behavior before a staged v3 upgrade, preserving
choices and permanent aliases. Existing Core policy, recall, protected coverage
and operations suites pass on this integration work. All-feature workspace Rust tests, fmt/Clippy, all-SDK build/check/smoke, packed
WASM/TypeScript and live 100-fixture SDK parity pass on the committed integration.
The full encrypted MV3 regression suite passes before commit; its clean
committed-head receipt and Small qualification remain to be refreshed.

Before merge: implement and qualify the approved capacity/recovery bounds,
approve remaining action/accounting semantics and legacy reuse errors; reconcile #148's final format and G3's all-SDK contract; rerun the
full installed/encrypted backup and native/browser qualification matrix. This
draft does not complete C1/C3, freeze snapshot compatibility or authorize a beta.3
release.
