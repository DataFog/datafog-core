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
