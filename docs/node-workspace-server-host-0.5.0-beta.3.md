# Native Node server-host qualification — 0.5.0-beta.3 draft

This is the focused Node reference-host slice of
[W6 / #170](https://github.com/DataFog/datafog-core/issues/170), stacked on native
Node #174. It uses synthetic values and an actual SQLite database shared by
independent workers. It is conformance code, not a shipping server vault or an
approval of production key management. Beta.2 release work remains independent.

## Ownership and atomic save

A request belongs to an authenticated tenant, workspace and opaque request ID.
The host authorizes that complete scope and resolves its key before decrypting
any snapshot. Workers reopen an independent Core handle from authorized saved
state. They never share live handles or treat the SDK handle's 32-plan ceiling
as a distributed lock. The reference test uses a fixed two-worker coordinator; production admission
control and limits across coordinators belong to the host deployment. The request
context supplies the response scope; provider output never chooses a workspace.

The reference store keeps canonical revisions as decimal TEXT and full commit
IDs, so SQLite's signed integer range cannot truncate Core's unsigned heads. A
real `BEGIN IMMEDIATE` transaction compares both expected revision and commit ID
and saves a state-changing candidate together. No-op candidates keep their head,
so independent no-op saves can both succeed. Delivery deduplication/admission is
a separate host duty and is not inferred from snapshot CAS. Busy waits are bounded; busy or stale failure
releases no text and does not activate a losing plan. Core preview and approval
are not persistence or delivery. The approval callback must return the literal
boolean `true`; strings, numbers and other truthy values reject without saving or
releasing. The staged Core plan captures input before approval, and a changed
presentation rejects rather than authorizing a rewritten preview.

Snapshot plaintext is encrypted with AES-256-GCM. Tenant, workspace, namespace and
complete head are authenticated as AAD. Only ciphertext, nonce/tag and opaque
routing/head metadata enter SQLite; source/request bodies and keys are not stored
or logged. The test explicitly supplies its host key on every reopen, including
process restart. An in-memory test key map is not a production key-persistence,
rotation or disaster-recovery design; those remain part of G4 #153.

After definite failed save, discard/close without release. After lost acknowledgment,
reopen durable state, obtain fresh review/approval and use a fresh operation ID.
A process can die after commit but before activation/delivery: durable mappings
survive while the absent delivery must not be invented. Cancellation or a changed
head blocks handoff before the release callback starts. Once release starts, the
host callback receives the `AbortSignal` and must cooperate by checking it before
its actual write; cancellation cannot retract an already performed write. A late
callback failure may follow delivery, so never infer "nothing sent" or retry
automatically from that failure. There is no atomic transaction spanning
SQLite and a remote provider; exactly-once remote delivery is not claimed.

## Stateless worker constraint

A worker without local memory must retrieve authorized durable mapping state or
receive an authenticated encrypted snapshot under an explicitly supported host
protocol. Resent user/chat history alone is not a mapping store and cannot resolve
unknown committed markers. Missing state must reject rather than guess, create a
replacement workspace or disclose marker-shaped text unchanged.

This reference qualifies the durable-store path. Client-carried snapshots require
separate freshness, replay, ownership and concurrent-update decisions; encryption
alone does not supply global CAS or prevent rollback. No history-only or
client-carried mode is silently inferred here. Provider tool/SSE parsing and a
LiteLLM adapter remain separate work; LiteLLM #22821 is closed/completed and only
motivates regression cases.

## Required evidence

- Two actual workers prepare state-changing candidates from the same head; exactly
  one saves/activates/releases, while the loser keeps its old head and releases
  nothing. Separate no-op workers both retain that head; CAS is not a delivery lock.
- Equal revision with a different commit ID fails; busy storage fails within the
  configured bound. Reopen/review/retry uses current mappings and fresh approval.
- Process death before commit rolls back; death after commit preserves mappings
  without claiming activation/delivery. Lost acknowledgment likewise releases
  nothing until reopened and reviewed.
- Tenant/workspace authorization precedes decrypt, AAD tampering rejects, two
  request contexts never swap outputs, and missing/history-only state fails.
- Cancellation before release and cooperative cancellation during a paused final
  write, failed writes and worker restart preserve the
  prepare/approve/save/activate/release boundary. Stored DB/WAL data contains no
  synthetic plaintext or key bytes. Failed-authentication tests also verify that
  transient decrypted update buffers are wiped when final tag validation fails.

The adapter uses pinned Node 24's built-in
[`node:sqlite`](https://nodejs.org/docs/latest-v24.x/api/sqlite.html), adding no
third-party database dependency. The native installed-package check must execute
these tests; documentation or a mocked compare-and-save does not qualify W6.

## Opt-in progressive response delivery

`restoreStoredResponseProgressively` is reference-host conformance code. The trusted
request supplies tenant/workspace, request ID and the full expected committed head;
provider output cannot select these. The host authorizes and decrypts durable state,
rejects a stale expected head before output, and opens a pinned native Core adapter.
It accepts decoded response byte chunks (or UTF-16 units when explicitly selected),
not SSE/provider framing. The existing whole-response restore path is unchanged.

Only successful Core calls reach `deliver`; a failed feed releases none of that
call. Every feed is incomplete, and only successful finalization supplies completion.
Late marker/encoding/resource failures preserve original ranges and prior returned
counts. Cancellation, iterator failure and callback failure stop without inventing a
complete response or automatically retrying. Cleanup closes native children and the
workspace. Earlier output is irreversible.

`delivered` measures text Core returned to this host, not confirmed network receipt.
A callback may write before failing, so its failure cannot prove non-delivery. The
host checks cancellation before feed and callback invocation; once invoked, callbacks
and provider iterators must cooperate with the supplied signal, including while
awaiting network I/O. This helper cannot retract remote output or interrupt arbitrary
uncooperative I/O. A same-head snapshot CAS remains unrelated to delivery admission,
idempotency or exactly-once network effects.

Installed tests use actual encrypted SQLite state and native Node restoration to
prove complete Unicode delivery, late unknown-marker and invalid-encoding failure,
stale expected-head rejection, cancellation between chunks and during a paused
cooperative write, and sanitized iterator/callback failures. Production key storage,
server-wide admission and client-carried state remain separate W6/G4 qualification.
