# C1/C3 operation records and counts — v3 review proposal

Draft for #147 / `0.5.0-beta.3`, reconciling #143/#158 with the committed
policy-v2 implementation. **Do not merge or publish this draft yet.** Sid approved
preserving every operation record: no deletion, sampling or summary replacement.
Lossless compression is separate postlaunch work in #165. Sid also approved
recorded-history-only counts, explicit unknown pre-upgrade history and per-record
numeric input/replacement deltas on October 29. He then approved the provisional
100,000-row ceiling with the final 256 slots reserved for recovery. The legacy reused-ID error compatibility is now specified below; final conversation
coordination and complete native/shared qualification remain open.

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

## Approved bounds and technical contract

### Capacity and recovery

The draft now enforces the approved count boundaries without evicting IDs. New
ordinary records stop at 99,744 total retained rows; only recovery may enter the
final 256 slots. At 100,000, further recovery fails explicitly while read/export
remain available. Legacy byte-preserving no-ops do not consume a slot. Known
record-producing calls preflight the count before cloning; legacy no-op detection
retains the existing Core semantics before deciding whether a record is needed.

**Approved October 29:** a provisional **100,000-record hard ceiling**, with the final 256
slots reserved for recovery and a small encoded-byte reserve for recovery
metadata. Ordinary recorded activity would stop at 99,744 total rows; a fresh
recovery could still retain every row and append its own record. This is a
bounded beta proposal, not unlimited history or an indefinite-recovery promise.
At the hard record/byte ceiling, writes including further recovery still fail;
read/export must remain available and the host must make the limit visible.
The current v3 encoder derives a conservative **7,699-byte reserve**: 256 maximum
30-byte recovery rows plus full-width head-revision, operation-array and operation-
count headers (19 bytes). Ordinary candidate encoding stops below that reserve;
recovery can use the actual 16 MiB cap. Encoding reuses the reviewed bounded private
storage helper so rejection/reallocation wipes intermediate plaintext. Valid older
vaults already near the byte cap remain readable/exportable and may hit the hard
byte limit before all recovery slots are usable. No data is trimmed to make room.

#148's future key9 conversation payload must remain unchanged by recovery, or its
extra recovery growth must extend this calculation. The conversation owner plans
that invariant, but the joint final v3 wire still requires coordinated tests before
G3 freeze. Count, byte-boundary, coverage and actual encrypted browser capacity tests pass;
committed-head receipts are refreshed below.

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

**Also approved October 29:** reads, restores and failures never mutate saved state.
A host may deliberately save a content-free failure or unchanged-preparation row
through prepare/export/durable-save/activate. Legacy calls without a timestamp
record unknown (`null`) time rather than inventing a historical clock. Legacy
no-op bytes remain unchanged. A batch records one operation, its input-member
count and replaced occurrences, including repeated values. Current policy saves
also append a proposed `policy_change` row with zero input/span deltas and unknown
time; combined review delegates to the same preparation path. One row per explicitly saved batch and unknown time for older calls preserve the
approved explicit-save boundary; compatibility tests cover their deltas and
no-op behavior. A policy no-op stays byte-preserving.

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

Legacy APIs consistently retain `invalid_operation_id` for every recognized
reuse, including current-head, mapping-issuance, settings and history-only IDs.
New explicit operation APIs return `operation_reused` for those same IDs. This
technical compatibility repair keeps the existing legacy error contract instead
of leaking a new error merely because an ID now exists only in retained history.
Every rejection preserves the complete saved bytes and head, including after
reopening. Existing records are never silently forgotten. Older snapshots cannot reveal removed/unrecorded IDs; the
runtime cannot reconstruct them or promise to detect unknowable replays.

## Proposed API and count definitions

Existing proposed Rust calls are `create_with_operation`, `prepare_with_operation`,
`prepare_batch_with_operation`, `prepare_recovery_with_operation`, `prepare_outcome`,
`operations`, `operations_page`, `history_coverage` and `counts`. WASM exposes matching camelCase calls. Native Python,
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

The read-only `historyCoverage()` (Rust `history_coverage`) view returns
`recordedHistoryOnly: true` and `preUpgradeHistory: "unknown"` for v1/v2 or an
upgraded history without its original create row. A new retained create-based
history reports `"not_applicable"`. This never claims that legacy no-op calls or
external activity were observed, and querying it never changes snapshot bytes.

Native Python/Node do not yet expose the operation proposal calls. The existing
shared workspace foundation remains mandatory for their installed tests; the new
operation profile is separately mandatory in WASM and MV3. Native operation
bindings and the all-SDK operation profile are follow-on work before C1/G3 is done.

## Approved-capacity proof and bounded read windows

The current implementation uses the approved 100,000 ceiling directly. The
`workspace_operation_capacity` example creates canonical synthetic fixtures at
99,744, 99,999 and 100,000 rows, reopens and re-exports them through Core and
measures actual requested allocations with an example-only counting allocator.
Thirty-two retained plans at 99,743 records require about 267 MB with compact
numeric deltas or 355 MB with maximum numeric deltas; temporary requested peaks
are about 268 MB/358 MB. Both return to zero additional requested bytes after
plans are dropped. Requested-byte accounting excludes allocator overhead, while
RSS includes runtime, caching and process state. This demonstrates a substantial
per-workspace pending-plan cost; it does not impose a global host memory budget.

The packed MV3 harness opens the 99,744-row boundary, rejects an ordinary write
without changing saved bytes, and exports a stronger backup. Password and
recovery-code imports each retain every old row in order and append exactly one
recovery row, including after browser restart. At 99,999 the last recovery reaches
100,000; at 100,000 read/restore/export and legacy no-ops still work, while another
recovery fails without creating a destination vault. Full-prefix hashes prove
retention beyond merely checking the first and last IDs.

`operations_page(offset, limit)` / `operationsPage(offset, limit)` is an additive
read-only window over the same rows. It returns `records`, `total`, `offset` and
`next_offset` / `nextOffset`; the cursor is null at the end. A limit must be
1–100,000 and the offset must be at most the total. An offset exactly at the end
returns an empty page. Invalid bounds return `resource_limit`. **The original
full `operations()` call is unchanged:** no rejection, truncation or deletion is
introduced. Rust borrows a slice, while JavaScript returns owned result objects.

Controlled browser measurements clone the same encrypted 99,744-row profile only
after browser shutdown, then open each clone in a fresh process with the same KDF,
identical seven-process topology and explicit JavaScript GC. Two repetitions show:

| Read mode | Retained JS heap | WASM linear memory |
| --- | ---: | ---: |
| No record view | 1.58 MB | 24.18 MB |
| Full 99,744-row view | 15.14 MB | 229.51 MB |
| Complete traversal in 1,000-row windows | 1.69 MB | 24.18 MB |

Both full and paged traversals visit every row in order with identical checksums
and numeric totals, leaving the snapshot byte-identical. Releasing the full JS
view returns its JS heap to about 1.58 MB; WASM retains its larger linear-memory
high-water mark. Summed process RSS was already about 1.07 GB before reading
history. Its incremental rise was about 219 MB for a full read versus 15 MB for
pages, and shared pages can be counted more than once. The earlier roughly
1.45 GB multi-workflow observation cannot all be attributed to history reads.
These are single-machine instrumented observations, not minimum-device claims,
unique physical memory measurements, peak browser memory or latency guarantees.

Reproduce with `cargo run -p datafog-core --release --example
workspace_operation_capacity -- target/operation-capacity`, build the packed MV3
harness, then run `node scripts/measure-workspace-operation-memory.mjs` and
`node scripts/test-workspace-extension.mjs`. The latter rebuilds the harness and
runs the full encrypted capacity and existing workflow regressions. Controlled
measurement receipts include before/after/released JS, linear-memory, per-process
RSS and process counts. [Checked-in approved-capacity receipts](evidence/workspace-operation-capacity/approved-100000/browser-capacity.json)
qualify clean committed `4dc78bd2f3367df03fc5180dd8090d51327c7d7e` (dirty false),
with native allocation and controlled browser measurements beside them. Later
documentation-only changes do not alter those qualified SDK sources. These
receipts do not claim the overall G3 or release gate has passed.

## Historical capacity experiments

Synthetic optimized native Rust runs on this Apple arm64 machine, with no model
or provider call. These are single-run observations, not stable-runner benchmark
gates, lower-hardware guarantees, Chrome timings or selected capacity defaults.
These historical 100k/300k builds temporarily changed the old 10k candidate and
restored it afterward. They predate the approved fixed-cap implementation and do
not describe its current admission behavior.

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
never automatically logged. Exact committed `4dc78bd2f3367df03fc5180dd8090d51327c7d7e`
passes fmt/Clippy, all-feature workspace tests, all-SDK build/doctor/check/smoke,
packed WASM/TypeScript, live 100-fixture SDK parity, the complete actual encrypted
MV3 regression/capacity suite and all 12 actual Small-model extension proofs.
All browser and allocation receipts report a clean source tree. A stale beta.1
wheel initially prevented live-parity setup; preserving it separately and using
the current built wheel passed without changing fixtures or product code.

Before merge: reconcile #148's final key9 format and G3's all-SDK contract.
The recognized-ID error matrix, explicit saved-policy rows and batch deltas remain
subject to ordinary technical review rather than a new retention/accounting decision. Native Python and
Node operation/history/page implementations and their shared qualification are
explicit prerequisites for complete X6/X7/X8 and C1/G3 claims. The approved
capacity and full-record retention do not settle ordinary confirmation reservation
for #148. This draft does not freeze snapshot compatibility or authorize a
`0.5.0-beta.3` release.
