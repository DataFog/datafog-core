# C2/R3 conversation context — review proposal

Target: `0.5.0-beta.3`; [#148](https://github.com/DataFog/datafog-core/issues/148).
This is a proposal, not an implemented API or an approved snapshot layout.
Operations/counts [#147](https://github.com/DataFog/datafog-core/issues/147),
all-SDK parity [#171](https://github.com/DataFog/datafog-core/issues/171) and
contract freeze [#154](https://github.com/DataFog/datafog-core/issues/154) remain open.

## First product decision: what counts as a handoff?

**Approved October 29:** record markers as issued into a conversation only after the
trusted host reports an observed successful handoff to that exact conversation.
Preparing, approving, saving mappings, copying text or filling a draft composer
alone must not be represented as a successful send. A copy-only workflow needs a
separately reviewed confirmation design; do not infer delivery from clipboard use.

This adds a second saved transaction after external handoff. The mapping save still
happens before any text leaves the host. If the later confirmation save is lost or
uncertain, the actual send may already have happened: do not resend automatically.
Reopen/reconcile saved confirmation state; conversation-aware restore fails closed
until the required issuance record is durably available. The ordinary compatibility
restore mode remains available under its existing documented contract.

Core cannot observe a browser or provider. The host's observation is an assertion,
not proof that a remote person/model received the text. The exact host event acquisition (for example, the sent message in the selected
conversation) still needs site-specific technical and shipping-host verification. An attempted click, an unacknowledged write and successful copying
are not equivalent observations. The confirmed-send/second-save sequence is
approved; manual confirmation, storage bounds and final wire/API details remain
separate review work.

## Recommended two-transaction flow

1. Capture the authorized workspace and exact conversation identity before review.
2. Prepare the input using current Core review/policy/overlap behavior. Obtain local
   approval; encrypt and save if the complete expected head still matches; activate.
3. Release the approved pseudonymized text to the selected host transport. An edit,
   changed conversation, cancelled operation or failed save cannot reuse approval.
4. After the host observes the agreed handoff success, prepare a confirmation using
   a fresh operation ID, the selected conversation and the committed markers that
   were actually in that handoff. Never accept new marker-to-original mappings.
5. Approve/save/activate that metadata transaction normally. Only committed
   confirmation state makes those markers eligible for conversation-aware restore.
6. On an uncertain confirmation acknowledgment, reopen and inspect saved state.
   Reconciliation must not silently repeat the external handoff or invent an event.

Core validates shape, committed marker membership, context identity, replay/bounds
and state transitions. It cannot validate the truth of a host observation or supply
an atomic transaction across a vault database and an external chat site.

## Conversation identity and private metadata

Propose a workspace-local pair of a host-defined surface identifier and the site's
opaque conversation ID. Compare both exactly; do not lowercase, trim, fuzzy-match
or derive identity from model text. The host supplies these through its trusted
request/tab registry, not provider-generated fields, arbitrary URLs or chat text.

The conversation record should retain its creating confirmation operation and the
canonical marker codes issued into it. Repeated confirmed handoffs add membership
without changing aliases or the first restored spelling. Individual operation
records remain preserved under the approved history rules. No raw conversation
text, original values, prompts, response bodies, titles, credentials or full URLs
belong in these records, errors, logs or qualification receipts. Opaque IDs and
marker membership are still private vault metadata, encrypted by the host.

Exact string bounds, conversation/member limits, issuance-event shape, timestamps,
count definitions and manual-confirmation support remain review proposals. Choose
and measure them before implementation/freeze. The approved 100,000-operation
ceiling and recovery reserve must not be silently repurposed as unlimited
conversation storage or weakened to make a new schema fit.

## Conversation-aware restoration

Propose an explicit scoped restore mode. Existing unscoped `restore`/`restoreReport`
and strict buffered R2 behavior keep their compatibility contract.

- Capture one immutable authorized conversation identity for the response. Never
  let a provider delta select a different workspace or conversation.
- Unknown/missing context rejects; do not silently fall back to unscoped restore.
- A valid workspace marker absent from that conversation's committed issuance set
  blocks restoration. Report content-free original byte/codepoint/UTF-16 ranges;
  never reveal its mapped value or substitute another conversation's identity.
- Valid issued markers restore normally. Existing unknown, damaged and bare-marker
  handling remains enforced; scoped validation cannot weaken it.
- Strict mode remains all-or-nothing. The separately approved progressive mode may
  retain earlier delivered chunks but stops incomplete on a later scope error.
- A scoped stream pins its committed context/head; existing close/change/reset and
  no-op behavior remains consistent with the shared streaming contract.

Error names, subtype/result fields and method spelling are technical contract work,
not implemented exports. Native Python/Node, browser WASM and Rust must expose and
actually execute the same final semantics; type-only declarations do not qualify.

## Snapshot, backup and required proof

Coordinate with the unmerged operation-v3 proposal: existing policy keys 1–7 retain
their meanings, operation/count fields are proposed at 8/10, and key 9 is reserved
for conversations. Do not create incompatible version-3 layouts. This document
assigns no wire types or unapproved numeric limits. New readers preserve released
v1/v2 bytes on read/export/no-op/discard; any real saved upgrade needs the approved
warning that beta.1/beta.2 cannot open upgraded state.

Required synthetic proof includes same/wrong/missing conversation, markers from
another conversation in the same workspace, repeated/batched handoffs, forged or
uncommitted codes, changed tab/context during approval, failed/uncertain mapping and
confirmation saves, cancellation and duplicate IDs. Exercise Unicode ranges,
strict/progressive chunk boundaries, restart and both password/recovery-code backup
imports. Missing a required SDK or scoped call must fail X8 qualification. Preserve
all records and aliases through recovery; test ordinary and recovery-reserve bounds
with the final coordinated schema. No live provider calls or real PII are needed
for these implementation fixtures; actual shipping-host security and G1 provider
behavior qualification remain separate gates.

## Concrete pending-send packet for review

This packet extends the approved sequence; it does **not** ask whether copying or
composer fill should count as sending. Neither counts. No conversation runtime is
implemented by this proposal. Its operations base is committed `22fb7618`; the
100,000/99,744/256 capacity implementation is still a draft under qualification.

### Why the first save needs a pending intent

Consider an existing conversation that already has issued marker A. A later
approved input contains A and new marker B. The mapping save succeeds; the site
accepts the send; saving the observed-send confirmation then fails or loses its
acknowledgment. The app crashes.

Without a durable pending record, reopening shows the old issued set containing
A and no indication of uncertainty. Restoring A would appear safe even though the
new send's confirmation was never reconciled. An in-memory flag cannot enforce the
approved pause after a restart or backup import.

**Recommendation:** the first explicit conversation-send preparation stages both
the mappings and a content-free pending intent in the same candidate. The trusted
host approves, durably saves and activates that candidate before attempting the
external send. This is an intention to send, never successful issuance. Preview,
ordinary preparation, a mapping-only save, copying and composer fill create no
confirmed send or automatic conversation membership.

An intent identifies the exact surface/conversation, the preparation operation and
batch input member. Core derives its canonical marker references from the **whole
approved output**, including already-committed follow-up markers, not just new
transformations. The confirmation takes that intent rather than accepting a new
caller-supplied marker-to-original mapping or an arbitrary replacement set.

After an observed success for that same intent and conversation, a second prepared
transaction records the observation, adds the intended issued references and
resolves the pending state. Only its durable save and activation count as a
confirmed handoff. A failed/discarded candidate leaves the saved pending state and
counts unchanged. Retrying a confirmation transaction never calls the transport.

### Bounded product question: how much does uncertainty pause?

**Recommend conversation-wide blocking.** While any durable send intent for the
selected conversation remains unresolved, all conversation-aware restore calls for
that conversation stop with a content-free pending-state error. This includes
older responses containing A, text containing only previously issued markers, and
ordinary text with no markers. Other conversations remain usable. Existing
unscoped compatibility methods keep their contract; the host must never silently
switch to them to bypass the pause.

The cost is explicit: a lost confirmation can pause earlier responses in that
conversation until the host reconciles it. The narrower alternative would let old
responses continue while blocking only the new response. That requires a trusted,
persisted request/response association that is not part of the current contract;
a provider field or model-generated identifier cannot establish that association.
Do not implement that alternative implicitly.

This is the remaining product effect to review, not a reopening of Sid's
confirmed-send decision. The code/fixture names below are proposals, not frozen
SDK exports.

### Reopening, cancellation and batches

- After a crash, failed confirmation save or uncertain acknowledgment, reopen the
  authorized saved state and inspect the exact intent. If the confirmation is
  already durably recorded, use that record. If it is still pending, scoped restore
  remains paused. Never invent an observed success or automatically resend.
- A trusted host that still has the actual matching observation may retry only the
  metadata confirmation against a fresh head. Missing evidence stays unresolved;
  there is no manual “I sent it” or copy-confirmation escape hatch.
- A definite observation that transport never sent may resolve an intent without
  increasing `confirmedHandoffs`. A generic network error or missing acknowledgment
  does not prove non-delivery. The exact not-sent API remains technical review work.
- Batch preparation must track intended input members separately. One member's
  observed send cannot confirm other unsent members. With conversation-wide
  blocking, another unresolved member for that conversation still pauses restore.
- Recovery preserves pending intents, confirmed membership, aliases and all
  operation records; it never marks an intent successful merely because it restored
  a backup. Restored pending state remains pending.

The pause applies to the current committed snapshot. Core cannot invalidate an
independent host's older handle or serialize a database and a chat site. The host
must load authorized current state, bind response context before restoration, and
own concurrent request/admission behavior; existing close/head-change rules still
apply to streams on a live handle. No distributed-lock or remote-receipt proof is
claimed.

### Wire and resource work before runtime

Coordinate one authoritative, unreleased v3 map with #162: keys 0–8/10 retain their
agreed meanings and key 9 receives the reviewed conversation payload. Do not ship
an absent-key9 and a different required-key9 format under competing v3 meanings.
Released v1/v2 read/export/no-op bytes remain unchanged. The current candidate
recovery reserve is 7,699 bytes; unchanged conversation metadata contributes zero
recovery growth, and a map10-to-map11 header stays one byte. A joint nonempty-key9
boundary measurement must verify the final schema rather than assume it.

The appendix must bound identity bytes, conversation count, pending intents and
membership/reference counts, and count the complete candidate against the 16 MiB
limit without eviction. Pending confirmation capacity also needs review: a first
save near the ordinary-record or byte ceiling must not promise a guaranteed second
save or consume the 256 recovery-only slots silently. An exhausted confirmation
write leaves the intent pending and scoped restore paused; read/export and explicit
compatibility behavior remain available under their existing contracts.

The first executable contract data is
[the existing-conversation crash fixture](../../fixtures/workspace/conversation-contract-draft.json).
It specifies expected future behavior only. Current SDKs do not execute these
commands; implementation must add actual Rust/WASM/Python/Node and MV3 qualification
before claiming acceptance or freezing the API.
