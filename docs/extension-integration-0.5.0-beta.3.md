# 0.5.0-beta.3: browser and native workspace integration

This guide describes the `0.5.0-beta.3` release candidate (Python `0.5.0b3`). Pin
the exact package and lockfile after publication and fresh registry verification.
The [versioned call/type inventory](contracts/launch-api-0.5.0-beta.2-beta.3.md)
and its compiled TypeScript/Python examples describe the actual shared surface.
The API and snapshot remain provisional beyond this functional prerelease.

Browser consumers load `@datafog/wasm` and await `init()`. Native Python and
`@datafog/node` call shared Rust in process. They expose lifecycle, preparation,
combined review, recall, choices, snapshots, recovery, operations, conversations
and restoration without a browser runtime. Existing
[beta.2 review semantics](extension-integration-0.5.0-beta.2.md) remain applicable.

## Saved operations and conversations

Mutations retain the same prepare → approve → export → encrypt/durable save with
the complete expected head → activate boundary. Approval must be literal `true`
/ Python `True`. A matching revision without its 16-byte commit ID is insufficient.
The host owns credentials, keys, request/tenant ownership and transport.

Use `prepareConversationSend` / `prepare_conversation_send` with the existing
combined-review inputs, a fresh operation context and the exact conversation
definition. The host creates a permanent private logical ID. The `siteId` field
is required: use null until the exact site ID is known, or provide the verified
existing-chat binding. The immutable `siteIdByteBudget` funds later binding growth.
Capture the plan's `sendIntents()` / `send_intents()` IDs before activation consumes
it. Save and activate pending state before releasing approved text to transport.

Only a trusted observed successful send for that saved exact attempt can call
`prepareObservedSend` / `prepare_observed_send`, followed by a second durable save
and activation. This establishes issued membership and, for an unknown first chat,
one-time site binding. `prepareDefinitelyNotSent` / `prepare_definitely_not_sent`
is for proved non-delivery, never a timeout or an ambiguous acknowledgment.
Uncertainty remains pending across crashes, reopen and backup/recovery. Do not
invent success, guess site identity or automatically resend to repair it.

Use the scoped `restoreInConversation` / `restore_in_conversation`, report and
streaming factories for conversation-aware responses. Any unresolved intent
pauses the whole selected conversation, including plain text and older replies.
Wrong-context markers reject with original ranges. The existing unscoped APIs
retain compatibility behavior; they are never a fallback around scoped checks.

Operation contexts use fresh random 16-byte IDs and untrusted integer host time.
Read `operations()` or explicit `operationsPage` / `operations_page` windows;
unknown legacy time/history stays unknown. Counts describe retained recorded
actions, not lifetime totals or remote-delivery proof. Unfunded ordinary activity
must preserve pending resolution credits within the ordinary ceiling. Funded
resolution remains possible after recovery up to the hard 100,000-row cap; all
writes protect remaining byte and checked-u64 revision credits.

## Strict default and opt-in progressive chunks

Scoped buffered restoration returns no feed text and releases the complete
response only after successful finalization. A report with issues has null text,
not a successful empty response. Progressive restoration explicitly accepts
irreversible earlier chunks. A later marker, encoding or resource failure returns
none of the failing call, retains prior delivery counts and stops incomplete.
Unfinished or oversized possible marker prefixes stop progressive mode rather
than flushing literal text; strict/default R2 behavior stays unchanged.

UTF-8 feeds use `Uint8Array` / Python `bytes`; UTF-16 feeds use `Uint16Array` /
exact uint16 lists. Both modes expose finalize/reset/close. Changed saved heads
invalidate children; successful same-head no-ops preserve them. Hosts own provider
parsing, cooperative cancellation, output callbacks and visible incomplete status.
Core counters describe returned text, not proof that a network consumer received it.

## Versions, limits and ownership

The coordinated inner v3 map includes operation history, conversation state and
counts. Old v1/v2 reads/no-ops remain exact. Warn before an approved real save or
destination recovery upgrades state: beta.1/beta.2 cannot read v3. Existing aliases
remain stable; old unshipped operation-only v3 layouts are not migration inputs.
Outer backup-file/KDF versions are separate. Reference-host password exports still
require reauthentication and the approved strong profile.

The 32-plan bound is per workspace handle. History/encoded-byte limits are finite;
measured large-graph memory use is not a low-hardware or global host-admission SLA.
Discard/close request-owned plans and streams. Native in-process execution does
not supply shared locks, persistence, authenticated anti-replay, a production vault
or a LiteLLM adapter. See the [Python contract](native/python-workspace.md),
[Node contract](native-node-workspace-0.5.0-beta.3.md),
[parity evidence](workspace-parity-0.5.0-beta.3.md) and
[backup field inventory](verification/workspace-backup-fields.md).

Real-provider trials, extended product pilots, Josh's shipping-host acceptance,
CSV and the permanent freeze continue after this functional beta. They are not
claims made by the release or reasons to delay it for an arbitrary feedback period.
