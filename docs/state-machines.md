# DataFog Core state machines

This is the state-oriented map of the shipped Core API and the repository's
Core host integrations. “Explicit” means the code stores an enum/status for the
state. “Implicit” means state is encoded by fields, ownership, durable records,
or ordered async steps. Detector-local parsing and pure transformations are
excluded unless their state crosses calls or affects lifecycle/retry behavior.

## Audit result

No reachable invalid state or transition was confirmed in this pass. The
highest-risk transitions—durable-save acknowledgment loss, lock during a late
save, retries, and competing imports—have regression coverage. The cases below
are recorded as verified invariants, not proposed bugs. Consequently this pass
has no confirmed-bug issue group or code fix to land. Re-audit these boundaries
when changing persistence or cancellation semantics.

## Core workspace and prepared changes

Sources: [`workspace.rs`](../crates/core/src/workspace.rs),
[`operations.rs`](../crates/core/src/workspace/operations.rs),
[`policy.rs`](../crates/core/src/workspace/policy.rs),
[`review.rs`](../crates/core/src/workspace/review.rs).

Workspace state is an optional committed snapshot plus a lifetime flag and a
read epoch. A prepared change owns an immutable candidate, its parent head, and
preview material. Review and policy preparation are the same candidate-plan
lifecycle; their domain-specific previews do not add another lifecycle state.

| Current state | Event / guard | Next state | Invariant |
|---|---|---|---|
| Workspace open at head H | prepare succeeds | Open at H + pending plan | Committed workspace is unchanged; candidate records parent H. |
| Pending plan | preview / inspect / export | Pending plan | Inspection does not activate or consume the plan. |
| Pending plan | `activate_saved(plan, H, C)`; same workspace lifetime, committed head H, plan parent H, candidate head C | Open at C; plan consumed | Host must persist the exact candidate before activation. A changed head invalidates existing read epochs. |
| Pending plan | discard / drop | Open at H; plan discarded | No committed mutation. |
| Open workspace | close / drop | Closed | Plans tied to its lifetime and calls on the handle fail closed. Closing does not erase an external saved bundle. |
| Open at H | another plan is activated first | Open at C; old plan remains pending but stale | Old plan cannot activate against C; parent/head check rejects it. |

Operation history is append-only and has a separate admission state encoded by
row count and operation kind. Ordinary writes stop at `MAX_ORDINARY_OPERATIONS`;
recovery and send resolution can use the reserved tail through
`MAX_OPERATIONS`. Pending conversation intents are included in admission
accounting. At capacity, write preparation fails; rows are never evicted. A
recovery/ordinary operation is still a prepared candidate and follows the same
save/activate lifecycle above.

## Conversation send intent

Source: [`conversation.rs`](../crates/core/src/workspace/conversation.rs).

| Current state | Event / guard | Next state | Invariant |
|---|---|---|---|
| No intent | prepare conversation send | Pending in candidate | Intent is not durable until the candidate is saved and activated. |
| Candidate pending | discard or failed persistence | No new durable intent | No send authority or marker issuance is created by preview. |
| Pending | exact candidate saved and activated | Pending durable intent | It survives reopen; scoped restore remains blocked for that conversation. |
| Pending | prepare observed-send resolution with matching conversation/surface/site, then save and activate | Observed sent | This is a host assertion; only this transition issues conversation marker membership. |
| Pending | prepare definite-not-sent resolution, then save and activate | Definitely not sent | No marker membership is issued. Timeout or missing acknowledgment is not evidence for this transition. |
| Terminal resolution | resolve again / replay | Rejected | The intent is absent from the pending map and resolution IDs are unique. |

The three exposed statuses are `pending`, `observed_sent`, and
`definitely_not_sent`. Unknown IDs reject. In a race, two plans can be prepared
from one parent, but only one candidate can activate; the loser conflicts and
must be prepared again from the new head. Save acknowledgment loss must not be
converted into a send resolution or transport retry.

## Restore streams

Sources: [`buffered_restore.rs`](../crates/core/src/workspace/buffered_restore.rs),
[`progressive_restore.rs`](../crates/core/src/workspace/progressive_restore.rs),
and SDK adapters under `bindings/*/src/workspace/stream.rs`.

Buffered restore has `active`, `finalized/failed`, and `closed` states. It pins
one committed head, accepts only its selected encoding, returns no text from
feed, and returns an all-or-nothing report at finalize. `reset` starts a fresh
response only while the pinned head is current; close/head invalidation wipes
the buffer. Repeated finalize/feed before reset rejects as inactive.

Progressive restore combines a response lifecycle with this parser machine:

| Parser state | Input | Next parser state / action |
|---|---|---|
| Text | ordinary character | Text; stage output |
| Text | marker-eligible word or `[` | Word or Bracket; retain ambiguous suffix |
| Word | word continuation | Word; retain |
| Word | overlong word | LongWord; emit literal safely |
| Word | `[` or other delimiter | Bracket or Text; validate/flush prior token |
| Bracket | first `[` after one `[` | Bracket; retain a possible opening delimiter |
| Bracket | ASCII letter after `[[` | Interior; begin marker candidate |
| Bracket | repeated `[` or other character | Bracket/Text; emit the leading literal bracket as needed, or flush and reprocess the character |
| Interior | marker token characters | Interior; retain bounded candidate |
| Interior | `]` | Closing |
| Closing | second `]` | Text; restore known marker, reject invalid marker, or keep allowed literal |
| Closing | other character | Text; candidate is no longer a complete marker |

Lifecycle is `active → complete` on successful finalize, or `active → failed`
on invalid encoding/marker, resource bound, close, workspace close, or head
change. Successful feeds may already have returned irreversible chunks. A
failed call releases none of that call's staged output, but cannot retract prior
chunks. `reset` starts a new response and zeroes its counters; it does not
retract already returned output. Malformed Unicode, feed after finalize, or a
stale head is terminal until reset where the API allows it.

## Model session and admitted request

Source: [`models.rs`](../crates/core/src/models.rs),
[`process.rs`](../crates/core/src/models/process.rs).

The model session's atomic state values are initializing (0), ready (1), failed
(2), and closed (3); `closing` is an additional admission fence. `ModelRequest`
is a separate single-slot lease whose drop releases admission.

| Current state | Event | Next state | Invariant |
|---|---|---|---|
| Initializing | worker starts and empty-output handshake succeeds | Ready | No request before successful handshake. |
| Initializing | load/handshake failure | Construction fails | Partially started child must be stopped. |
| Ready, no request admitted | `prepare_request` succeeds | Ready, one request admitted | Concurrent admission returns `model_busy`; dropping request releases slot, including pre-execution errors. |
| Ready | inference/protocol/output conversion fails | Failed | Worker is retired; subsequent model calls reject. |
| Ready | close requested | Closing, then Closed | New work is rejected; close stops worker within cleanup deadline. |
| Any live state | cleanup failure | Failed/cleanup error | Cleanup failure is observable and does not silently permit reuse. |

Rule-only routed scans do not reserve the model worker, but still enforce
session lifecycle, input limit, cancellation, and output bound.

## Browser FogVault owner

Source: [`integrations/fogvault-browser/index.mjs`](../integrations/fogvault-browser/index.mjs).

The owner combines `open/closed`, `idle/busy`, an epoch fence, a saved receipt,
and at most one pending plan wrapper. The plan wrapper is weakly held while the
owner retains one cleanup handle, so garbage collection cannot strand a Core
candidate.

| Current state | Event / guard | Next state | Invariant |
|---|---|---|---|
| Opening | receipt, record identity, binding, Core namespace/head all agree | Open idle | Read is fenced by before/after receipt generation and digest. |
| Open idle | prepare succeeds | Open idle with one pending plan | Second live plan is refused; abandoned wrapper candidate is reclaimed before another preparation. |
| Pending plan | commit without literal `approved: true` | Pending plan | No save or activation. |
| Pending plan, approved | candidate identity validated; exact save acknowledged; Core activation succeeds | Open idle at new head/receipt | Receipt and record revision advance only on validated save acknowledgment; previews return only after activation. |
| Pending plan, stale head | next plan/plan use detects mismatch | Open idle, plan consumed | Stale plan cannot be saved or activated. |
| Open | lock/close/vault lock or close | Closed, epoch incremented, Core revoked | Late async replies cannot reactivate owner or release previews. |
| Save outcome unknown / unexpected commit-path error | commit catch | Closed | No replay through old plan; caller reopens/reconciles and prepares fresh work. |

`busy` rejects reads/preparations during commit. Concurrent commit/lock races
are fenced by epoch checks before activation. Durable save may have succeeded
when lock wins; the owner closes and the saved record is recovered by reopening,
never by trusting the late reply.

## Native FogVault owner and binding handles

Sources: [`integrations/fogvault-native/src/lib.rs`](../integrations/fogvault-native/src/lib.rs),
[`integrations/fogvault-node/src/lib.rs`](../integrations/fogvault-node/src/lib.rs),
and [`integrations/fogvault-python/src/lib.rs`](../integrations/fogvault-python/src/lib.rs).

The native `Owner` has the same durable boundary as the browser owner: an open
vault/Core pair, pending changes, exact save, activation, lock, and reconciliation.
Node/Python wrappers store multiple opaque plan handles (Node caps them at 32),
while binding stream handles wrap the Core buffered/progressive machines above.

| Current state | Event / guard | Next state | Invariant |
|---|---|---|---|
| Open owner | prepare | Open owner with opaque candidate handle | Candidate is scoped to this owner; inspection does not commit it. |
| Open owner with candidate | commit without approval or invalid parent/head | Open owner; commit rejected | No durable mutation is accepted for a stale candidate. |
| Candidate, approved | save confirmed, then Core activation | Open owner at saved head; candidate consumed | Returned previews follow durable save and exact activation. |
| Candidate, uncertain durable result | reconciliation by request ID | Core remains closed; receipt identifies durable outcome | Reconciliation is metadata-only; caller must drop/reopen and prepare fresh work. |
| Open owner | lock | Locked/reopen-required; plans and streams cleared | No later call through the old owner can use its Core state. |
| Open/locked owner | close/drop | Closed | Reopen requires authenticated vault state. |

Native host tests cover rollback, lost acknowledgment, exact request
reconciliation, stale plan rejection, and lock behavior in
`integrations/fogvault-native/tests/native.rs` and the installed Node/Python
qualification suites.

## Legacy browser backup import

Source: [`integrations/fogvault-legacy-browser/index.mjs`](../integrations/fogvault-legacy-browser/index.mjs).

This import-only adapter authenticates/decrypts a supported archive into a
staged session, then delegates approved installation to the current browser
owner. It has `opening`, `staged idle`, `staged busy`, and `closed` states.

| Current state | Event / guard | Next state | Invariant |
|---|---|---|---|
| Opening | authentication, Core decode, and identity/head checks succeed | Staged idle | Archive plaintext is retained only in the private session and is wiped on close. |
| Opening | abort, authentication/decode/binding error | Closed | Open failure cleans up and locks any attached destination vault. |
| Staged idle | preview | Staged idle | Preview does not install or authorize. |
| Staged idle | commit without literal approval | Staged idle | Rejected before destination mutation. |
| Staged idle | approved commit | Staged busy | A second commit rejects as `operation_in_progress`. |
| Staged busy | import succeeds and source session remains live | Closed source session; returned current owner | Legacy session locks/clears only after current owner installation succeeds. |
| Staged busy | known pre-commit failure with successful cleanup | Closed | Retry requires reopening/authenticating the archive. |
| Staged busy | source abort/close or uncertain save outcome | Closed; `import_requires_reopen` | Request ID is retained for reconciliation; no old-session retry. |
| Any staged state | close / abort signal | Closed | Abort is an out-of-band fence; late completion cannot return an unlocked owner. |

This retry behavior differs from ordinary workspace plan approval: even a
known rolled-back import failure discards the staged decrypted source. It is
tested and fail-closed; this audit did not establish a correctness bug.

## Included extension host

Source: [`examples/workspace-extension/src/broker.js`](../examples/workspace-extension/src/broker.js),
[`rpc.js`](../examples/workspace-extension/src/rpc.js), and
[`storage.js`](../examples/workspace-extension/src/storage.js). This is the
repository's synthetic integration harness, not the Core library API.

The broker encodes locked/unlocked in `session`, one pending Core plan, one
pending import, a monotonically changing `generation`, and a single `activeDone`
operation gate. `run` serializes ordinary broker commands. `cancel` and `lock`
are deliberate out-of-band invalidations: they increment generation, abort
work, stop workers, clear staged state, and await active work; lock also revokes
the vault generation.

| Current state | Event | Next state | Invariant |
|---|---|---|---|
| Locked | create/unlock completes and record/Core binding validates | Unlocked | Every async continuation checks captured generation and saved record identity. |
| Unlocked, no plan | prepare | Unlocked with pending plan | Preparing a replacement discards the previous candidate first. |
| Pending plan | pre-save storage failure | Pending plan retained | Normal approval can be retried; no content is released. |
| Pending plan | exact save acknowledged and Core activation succeeds | Unlocked at new record/head; no pending plan | Only then are previews returned. |
| Pending plan | failure after durable save / lost acknowledgment | Locked worker state, no pending plan | Never resend/release based on ambiguous acknowledgment; reopen saved state. |
| Import staged | approve; pre-save storage failure or abort | No staged import | The current implementation clears the staged import even when storage rolled back; retry requires reopening/re-authenticating the backup. This is an intentional, tested fail-closed transition, though less retryable than ordinary plan approval. |
| Import staged | exact save acknowledged | Import committed and remains locked | Import clears staged worker/state; after-save loss is reconciled by fresh unlock. |
| Any state | cancel | Invalidated generation, locked/unlocked status preserved as configured | Late operations fail their generation guard. |
| Any state | lock | Locked and vault access generation revoked | Existing session and workers cannot be reused. |

RPC client calls are `pending → resolved/rejected`; worker error, message error,
or explicit stop rejects all outstanding calls and makes the client stopped.
IndexedDB `saveRecord` is a single read/compare/write transaction: it either
commits the vault/record together or aborts without partial storage mutation.

## Existing evidence and follow-up

The following evidence exercises reachable retry/concurrency boundaries:

- Core lifecycle, stale plan activation, close, and stream invalidation:
  `crates/core/tests/workspace*.rs` and
  `scripts/native-node-workspace-conformance.mjs`.
- Conversation pending/reopen/observed-send and metadata-only retry:
  `crates/core/tests/workspace_conversations.rs`,
  `scripts/native-conversation-host-node.mjs`, and
  `examples/workspace-extension/src/conversation-proof.js`.
- Save rollback, late save after lock, lost acknowledgment, and competing host
  writers: `scripts/node-workspace-host-conformance.mjs` and
  `integrations/fogvault-browser/test/browser.mjs`.
- Import cancellation before and after save, rollback, lost acknowledgment,
  and competing imports: `scripts/test-workspace-backup.mjs`.
- Model busy admission, cancellation, worker death, and close interruption:
  `scripts/model-fault-conformance.py` and `crates/core/src/models/tests.rs`.

No confirmed bug was found, so there are no fix issues to group or regression
tests to add in this audit. A new suspected failure should be filed against the
machine and transition above, with a reproducer at the listed integration seam.
