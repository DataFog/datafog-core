# Conversation wire/API — Core/WASM implementation draft

Technical proposal for [#148](https://github.com/DataFog/datafog-core/issues/148),
built on the merged operation-history contract. The [send/pause decision](workspace-conversation-context.md)
is approved. [Ordinary confirmation reservation](conversation-confirmation-capacity.md)
was approved October 29. Core and WASM now implement the reviewed reservation/identity contract in this
draft. Native Node/Python conversation wrappers, final encrypted/MV3 qualification,
resource measurements, independent review and API freeze remain pending.

## One coordinated unreleased wire

The candidate canonical v3 map has **11** keys: existing keys 0–8 and 10 keep their
operations/policy meanings; key 9 becomes a mandatory four-element array:

| Position | Rows, exact tuple order | Meaning |
| --- | --- | --- |
| 0 | `[logicalId16, surface, siteIdOrNull, siteIdByteBudget]` | Immutable private logical identity and site-ID UTF-8 budget; nullable site ID binds exactly once after trusted observed send. |
| 1 | `[prepareOperationId, inputIndex, contextIndex, markerRefs]` | Active saved intents; `markerRefs` is a sorted unique array of `[label, code]`. |
| 2 | `[resolutionOperationId, prepareOperationId, inputIndex, contextIndex, outcome, markerRefs]` | Retained terminal resolutions and exact original intended references; proposed outcome `0=observed_sent`, `1=definitely_not_sent`. |
| 3 | `[contextIndex, label, code, firstConfirmationOperationId]` | Unique issued membership and the first saved confirmation establishing it. |

IDs are existing 16-byte operation IDs. Labels and six-character codes use existing
canonical marker grammar. Neither a caller-supplied replacement set nor arbitrary
original values enter confirmation. Context identity compares exact valid UTF-8
strings: no trimming, case folding, URL interpretation or Unicode normalization.
A surface and any bound site ID must be nonempty; no site-specific ASCII grammar
is invented. The host supplies the immutable 16-byte logical ID before review and
substantiates the persisted site-ID byte budget before claiming first-chat support.
A matching successful-send confirmation may fill a previously null site ID once;
a changed/oversized ID or duplicate exact surface/site binding rejects atomically.
Core cannot prove a host observation or its declared acquisition/length contract.

Context rows are created only with a saved intent and never removed/reindexed.
Intent identity is `(prepareOperationId,inputIndex)`; one batch member has one
context and one observed send. Derive references from the **whole approved output**
including valid preexisting markers, deduplicated canonically. Preparation rows
retain their existing input/transformation totals. A resolution appends its own
ordinary operation row and terminal tuple, removes exactly its active intent, and
adds membership only for an observed successful send. Both outcomes count as real
operations; only observed-sent increments `confirmedHandoffs`. Terminal tuples and
operation rows remain forever; this is content-free event history, not archived
prompts or per-message text. No resolution can change aliases or first spellings.

Terminal `markerRefs` must retain the exact canonical intended set after its
active intent is removed. Without those references, a decoder cannot cross-check
which first saved confirmation established an issued membership. Define a context's
creating operation as the earliest retained preparation row referenced by that
context's active or terminal members, using operation **row order**, not lexical ID
order, with input index as the tie-break within a preparation. Context array order
must follow that creating-member order. Reject orphan contexts. Each referenced
preparation must have the expected
preparation kind and valid input count. For each membership, recompute the earliest
observed-sent terminal containing that `(label,code)` for that context and require
its resolution ID to equal `firstConfirmationOperationId`; require that resolution
to reference a matching retained resolution-kind operation row. Repeated sends do
not change the first confirmation. No membership can be justified by a not-sent
terminal, unrelated operation, arbitrary counter or only an active intent. These
are structural saved-history checks; they cannot prove a host observation truthful.

Existing operation-kind codes 0–7 are unchanged. Candidate codes are
`8=send_confirmed`, `9=send_not_sent`, `10=conversation_prepare`. Every prepared
batch member has exactly one active-or-terminal intent; the decoder checks
`conversation_prepare.preparedInputs` against those members. Each confirmation
transaction resolves exactly one member and contributes one confirmed handoff,
independent of alias count. Not-sent/pending/preview/recovery contribute none. Recompute counts from retained operation
rows plus corresponding terminal tuples; reject claimed success totals without
matching saved success. No active intent, duplicate terminal event, forged
membership or resolution replay may be accepted simply because counters match.

This map is a **candidate**, not a second competing meaning of v3. Before freeze,
#162 must incorporate the final key9 layout and use it consistently for every
new/saved v3. Current map10 experiments are development fixtures, not published
vaults; this candidate decoder accepts only canonical map11 for v3. Existing released v1/v2 read/export/no-op/discard remain byte-exact. A real
saved upgrade uses the approved prior warning that beta.1/beta.2 cannot open it.
If map10 v3 ships first, the conversation layout needs a subsequent version rather
than changing the meaning of an already released v3.

## Bounded decoding and storage

The authority for limits is the existing Core contract: 16 MiB complete snapshot,
100,000 retained operation rows, 99,744 ordinary rows, 256 recovery-only rows,
32 inputs per preparation, 20,000 workspace aliases, 32-byte labels and existing
six-character codes. This proposal adds **no guessed per-chat or ID quota**.

Derive context/intent/resolution bounds from those facts: each context must be
referenced by an actual retained preparation member or its resolution; each
`inputIndex` is below that operation's `preparedInputs` and 32; each member has
exactly one active-or-terminal state. Thus their cardinality is at most the sum of
retained preparation-input counts (itself at most `100000 × 32`), and terminal rows
also cannot exceed retained resolution rows. Membership per context cannot exceed
20,000 actual aliases. Global membership and identity bytes must fit the **same
complete** 16 MiB encoded state; they are not separate 16 MiB allocations.

Before allocation, bound every declared CBOR length by remaining input bytes and
its tuple's minimum encoded size; use checked arithmetic for aggregate byte/count
budgets. Borrow/decode identity slices only after the complete bundle-size check;
account owned copies and indexes in implementation memory qualification. Explicit
API input identities likewise consume one aggregate existing bundle-sized budget,
with bounded failure before copying. Bound scans by encoded rows/references; do not
multiply maximum contexts by maximum aliases to preallocate a Cartesian product.
Shorter practical host limits can be documented by that host; Core must not silently
introduce them as a universal conversation policy. The final implementation still
needs measured heap/CPU evidence before these structural bounds are called qualified.
Use the approved reservation accounting to derive any stronger outstanding-intent/index budget
from its funded ordinary resources rather than adding an arbitrary quota. A wire
cap is not proof of bounded practical heap usage or acceptable processing time.

Reject indefinite/noncanonical/trailing/unknown fields; duplicate identities,
intents, terminal keys or memberships; out-of-range indices; active-and-resolved
duplicates; absent/wrong-kind referenced operations; invalid marker grammar or
unallocated references; a member confirmed by a different context/intent; and
inconsistent counts. Validate before mutation/index publication. An error leaves
the saved bytes/head unchanged and emits no original values in diagnostics.

Recovery changes only its existing head/row/count fields. It preserves **all**
key9 identities, pending intents, terminal events and memberships byte-for-byte;
it never resolves uncertainty. Consequently unchanged key9 contributes zero
recovery growth. The 7,699-byte recovery reserve still requires joint final-encoder
boundary tests. Confirmation-byte credits and recovery interleaving follow the
approved capacity contract and are recomputed from active intentions and history.

## Thin Core/WASM surface

Rust uses snake_case; current WASM uses the camelCase methods below. Native
Python/Node conversation wrappers must follow shared Core in separate slices. No
new method is implied to be frozen or present in the released beta.2 packages.

| Current WASM call (native follow-on pending) | Inputs / result | Ownership and failure |
| --- | --- | --- |
| `prepareConversationSend([{input,conversation}], operation)` | Current review input plus immutable conversation identity per member; opaque prepared change with intent handles. | Existing prepare/save/activate; no transport call or success count. No new implicit alias allocation beyond current reviewed input. |
| `sendIntentStatus(intent)` | Exact preparation ID/member; pending or retained terminal event. | Read-only, owned content-free result; unknown intent rejects. Useful after lost acknowledgment; never sends. |
| `conversation(logicalId)` / `conversationForSite(surface,siteId)` | Known context and pending/issued counts. | Read-only; unknown context rejects even for plain text. No full private membership dump required by restoration. |
| `prepareObservedSend(intent,logicalId,surface,siteId,operation)` | Exact active intent and trusted host's matching successful-send observation; prepared change. | Metadata-only; caller cannot replace intended marker set. Replay/conflict rejects; persistence required before issuance. |
| `prepareDefinitelyNotSent(intent,logicalId,operation)` | Exact active intent plus trusted definite non-delivery; prepared change. | Only trusted definite non-delivery; generic network error, missing acknowledgment or manual assertion is insufficient. Core cannot prove host evidence. |
| `restoreInConversation(text,logicalId)` / `restoreReportInConversation(...)` | Existing strict restore shape with fixed context. | All-or-nothing; no automatic unscoped fallback. |
| `bufferedRestoreInConversation(logicalId,encoding)` / `progressiveRestoreInConversation(...)` | Existing strict/progressive child lifecycle and encoding contracts. | Owned child pins head/context; parent close or changed head invalidates it; same-head no-op preserves it. |

Opaque intent references are owned copies of IDs/member index, not pointers into a
plan; they survive committed reopen but confer no authority without current Core
state and authorized host context. JSON/TypeScript errors retain existing exact
u64 head semantics, input/finding indices and original byte/codepoint/UTF-16 issue
ranges; Python exposes its native integers/bytes without lossy conversion. Native
Node/Python must reuse the same Core decision and child ownership as WASM.

Candidate content-free errors: `conversation_unknown`, `conversation_pending`,
`invalid_send_intent`, and marker issue `not_issued_in_conversation`. Scoped pending
check occurs before parsing/releasing any response, including plain text. A valid
workspace marker absent from the selected context is a scope issue; existing
unknown/damaged/bare syntax handling remains intact. Strict returns no text on an
issue. Progressive preserves only previously delivered counters/text and ends
`complete:false`; it cannot retract prior chunks. There is no mid-stream context
switch, provider-controlled identity, independent-handle invalidation or distributed
admission guarantee. Current-state loading/CAS and transport observation remain host
responsibilities.

## Reservation accounting and remaining qualification

The Core admission proof derives `P`, one outstanding ordinary row/revision credit
per active intent. Unfunded ordinary writes must fit `T + P <= 99744`; recovery
and exact funded resolution fit `T + P <= 100000`. Only a matching active intent
can spend its credit. A funded resolution does not reset the recovery pool.

An unfunded ordinary save funds the existing derived 7,699-byte/256-record recovery
pool. Later recovery rows consume their actual encoded row and numeric-header
growth; unused residual is freed after all 256 allowances are used. Separate
confirmation credits conservatively cover the whole retained terminal, maximum
timed operation, worst new membership/binding and header growth, without assuming
a removed pending row will pay for anything. Every save preserves remaining credits
and recovery pool in the complete 16 MiB snapshot. The decoder derives/revalidates
these pools and checked available u64 increments; it trusts no claimed credit totals.
A valid legacy head near MAX still reads/exports/no-ops unchanged, but a new send
rejects before transport unless both saves and promised increments fit.

Current actual Core tests cover partial batch/reopen/recovery/scoped offsets,
100,000 retained rows with all 256 recoveries and a later funded confirmation,
MAX/MAX-1/MAX-256 rejection and the exact MAX-258 two-save/recovery sequence.
The installed WASM/Chromium suite executes the new public methods and TypeScript
consumers. The exact 16 MiB edge test rejects one extra byte before sending, then
completes all 256 recoveries and the funded maximum-site-ID confirmation.
Worst-case heap/CPU and encrypted-host qualification are still in progress.

The earlier [encoder-only receipt](evidence/conversation-wire-growth.json) at
`ffa59ba` remains historical evidence for unchanged-field growth only. It did not
qualify a conversation decoder or final admission; current real Core tests replace
the prototype map-carry experiment. No provider acceptance is inferred.

The [draft adversarial fixtures](../../fixtures/workspace/conversation-contract-draft.json)
describe same-chat partial batches, uncertain sends, wrong context, malformed
relations and scoped-stream stops. They need actual Rust/Python/Node/WASM execution,
reference-host crash/CAS tests, password/recovery backup, nonempty-key9 resource
edges and installed Chrome proof before release. The existing unscoped R2 and
foundation/stream parity profiles must continue to pass unchanged.
