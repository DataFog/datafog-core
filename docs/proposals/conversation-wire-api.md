# Conversation wire/API packet — not runtime

Technical proposal for [#148](https://github.com/DataFog/datafog-core/issues/148),
stacked on committed operations `d5584e9`. The [send/pause decision](workspace-conversation-context.md)
is approved. [Ordinary confirmation reservation](conversation-confirmation-capacity.md)
is still awaiting a separate decision. No reservation ledger, send admission,
conversation SDK export or passing conversation conformance is implemented here.

## One coordinated unreleased wire

Propose one final v3 map with **11** keys: existing keys 0–8 and 10 keep their
operations/policy meanings; key 9 becomes a mandatory four-element array:

| Position | Rows, exact tuple order | Meaning |
| --- | --- | --- |
| 0 | `[surface, conversationId]` | Workspace-local conversation identities, stable append-order indices. |
| 1 | `[prepareOperationId, inputIndex, contextIndex, markerRefs]` | Active saved intents; `markerRefs` is a sorted unique array of `[label, code]`. |
| 2 | `[resolutionOperationId, prepareOperationId, inputIndex, contextIndex, outcome]` | Retained terminal resolutions; proposed outcome `0=observed_sent`, `1=definitely_not_sent`. |
| 3 | `[contextIndex, label, code, firstConfirmationOperationId]` | Unique issued membership and the first saved confirmation establishing it. |

IDs are existing 16-byte operation IDs. Labels and six-character codes use existing
canonical marker grammar. Neither a caller-supplied replacement set nor arbitrary
original values enter confirmation. Context identity compares exact valid UTF-8
strings: no trimming, case folding, URL interpretation or Unicode normalization.
An identity must contain two nonempty strings; no site-specific ASCII grammar is
invented. The host obtains immutable identity before review from its trusted
request/tab state. New chats whose stable ID is unavailable before send remain a
shipping-host integration gap; provisional IDs or post-send rebinding need review.

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

New operation-kind codes must be allocated jointly with #162 before runtime; no
wire number is assigned by this packet. Recompute counts from retained operation
rows plus corresponding terminal tuples; reject claimed success totals without
matching saved success. No active intent, duplicate terminal event, forged
membership or resolution replay may be accepted simply because counters match.

This map is a **candidate**, not a second competing meaning of v3. Before freeze,
#162 must incorporate the final key9 layout and use it consistently for every
new/saved v3. Current map10 experiments are development fixtures, not published
vaults. Existing released v1/v2 read/export/no-op/discard remain byte-exact. A real
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
boundary tests. Confirmation-byte credits and recovery interleaving are in the
separate pending decision; nothing here promises capacity for the second save.

## Proposed thin SDK surface

These names belong only in proposal examples until implemented. Rust/Python use
snake_case; Node/WASM use the camelCase forms below. Shared Core owns semantics.

| Proposed Node/WASM call | Inputs / result | Ownership and failure |
| --- | --- | --- |
| `prepareConversationSend(inputs, operation)` | Current review input plus immutable conversation identity per member; opaque prepared change with intent handles. | Existing prepare/save/activate; no transport call or success count. No new implicit alias allocation beyond current reviewed input. |
| `sendIntentStatus(intent)` | Exact preparation ID/member; pending or retained terminal event. | Read-only, owned content-free result; unknown intent rejects. Useful after lost acknowledgment; never sends. |
| `conversationState(identity)` | Known context and pending/issued counts. | Read-only; unknown context rejects even for plain text. No full private membership dump required by restoration. |
| `prepareObservedSend(intent, identity, operation)` | Exact active intent and trusted host's matching successful-send observation; prepared change. | Metadata-only; caller cannot replace intended marker set. Replay/conflict rejects; persistence required before issuance. |
| `prepareDefinitelyNotSent(intent, identity, operation)` | Exact active intent plus trusted definite non-delivery; prepared change. | Technical proposal: generic network error/missing acknowledgment/manual assertion is insufficient. Root review must settle host evidence contract before exposure. |
| `restoreInConversation(text, identity)` / `restoreReportInConversation(...)` | Existing strict restore shape with fixed context. | All-or-nothing; no automatic unscoped fallback. |
| `bufferedRestoreInConversation(identity, encoding)` / `progressiveRestoreInConversation(...)` | Existing strict/progressive child lifecycle and encoding contracts. | Owned child pins head/context; parent close or changed head invalidates it; same-head no-op preserves it. |

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

## Evidence and remaining implementation gates

Run the encoder-only experiment with:

```sh
cargo run --release -p datafog-core --example workspace_operation_capacity -- target/conversation-wire-growth/capacity
cargo run --release -p datafog-core --example conversation_wire_growth -- target/conversation-wire-growth/capacity
```

It wraps **actual Core** before/after recovery exports in the proposed map11 with
an unchanged synthetic nonempty key9. It explicitly checks that current Core
rejects map11. Equal measured growth proves only the unchanged-field byte equation;
it does not prove decoder validity, send behavior, reservation admission, encrypted
reopen, or capacity after conversation implementation. No mock SDK pass is claimed.
Adding key9 to a map10 bundle near the byte ceiling can itself exceed admission;
equal recovery growth does not make such a candidate admissible. The entire final
map11 must fit the existing ordinary byte limit before any saved send preparation.

The [clean-commit receipt](evidence/conversation-wire-growth.json) at `ffa59ba`
measured a 66-byte synthetic payload (67 bytes including key9). At 99,744 and
99,999 existing rows, real Core recovery added 30 bytes; the carried map11 also
added 30 bytes in each case. The extra recovery growth was zero. These snapshots
were approximately 3.3 MB, **not** a final 16 MiB conversation admission test.

The [draft adversarial fixtures](../../fixtures/workspace/conversation-contract-draft.json)
describe same-chat partial batches, uncertain sends, wrong context, malformed
relations and scoped-stream stops. They need actual Rust/Python/Node/WASM execution,
reference-host crash/CAS tests, password/recovery backup, nonempty-key9 resource
edges and installed Chrome proof before release. The existing unscoped R2 and
foundation/stream parity profiles must continue to pass unchanged.
