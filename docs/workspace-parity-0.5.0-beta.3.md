# Native workspace parity (active draft)

[#171](https://github.com/DataFog/datafog-core/issues/171) compares deterministic
Core workflows across Rust, actual installed native Python/Node and browser WASM.
`fixtures/workspace/sdk-contract-v1.json` specifies namespaces, operation IDs,
source/configuration, mutation order and expected errors. Canonical INNER snapshot
bytes, markers, full lossless heads, all source/output ranges, metadata, policy,
recall/review/restore reports and failures must match. Host ciphertext, nonces and
provider engine accuracy are outside this exact contract comparison.

The current profile covers the committed launch workspace surface. Missing
mandatory bindings/methods, failed or malformed calls, unexpected codes and any
mismatch are failures; declarations/mocks/skips do not qualify. A fixture's
`expectCode` permits only that documented operation failure, not a missing method
or harness failure. Result keys normalize only representation (Python's exact
revision integer to its decimal wire string, copied bytes to lowercase hex),
without discarding offsets, choices, flags or errors.

Conversation provenance and operation/history access run as separate mandatory
four-SDK profiles. Existing Python/Node encrypted server
CAS/process qualification is now separately mandatory, with explicit two-host
applicability; it does not certify four SQL hosts or shipping-host security. Strict and opt-in progressive restoration
now run as a second mandatory four-SDK profile, alongside the launch foundation. This foundation profile must never be
presented as full 0.5.0-beta.3 or real-data security certification. The foundation native wrappers are committed. This focused stack consumes the
committed Python/Node operation wrappers from #186/#187 for actual qualification;
it does not change the published beta.2 release.

## Run and inspect

After `cargo dfv build` and `cargo dfv doctor`, run:

```sh
cargo dfv parity --workspace --json
```

This executes all four SDKs with no sampling or single-SDK bypass. The blocking
verification workflow runs it after the normal scan parity/smoke gates. Protocol
failure tests reject missing methods, malformed/incomplete results and a restored
prefix reported as success alongside marker issues. A dry run explicitly reports
`pass: false` and `executed: false`, even though the command itself can exit normally.

Reports live under `target/dfv/runs/<timestamp>/workspace-parity/`: raw canonical
per-SDK outputs, the exact fixture, Python wheel/Node/WASM tarballs, SHA-256 receipts,
runtime versions, source commit/dirty state, doctor data and final pass/failure.
Python selection requires exactly one wheel matching the current package/version
in `bindings/python/pyproject.toml`, then checks the installed distribution version.
Historical wheels remain retained; they cannot replace the current wheel or make
an otherwise valid current build ambiguous. Multiple current candidates or a
missing/invalid pin fail rather than selecting an arbitrary artifact.
The CLI never deletes reports. Temporary installed consumers and browser profiles
are cleaned separately. Any process/runtime/parse failure prevents a complete
comparison and returns nonzero without retaining arbitrary child diagnostics.

The launch foundation retains 17 scenarios and 156 real operation steps, including every
current workspace error code, Unicode/source metadata, correction conflicts,
failed activation receipts, protected policy, repeated operation IDs, atomic batch
failure, strict marker reports, canonical reopen/recovery, legacy saved-format upgrade and a revision above JavaScript's exact numeric range. Only representation defaults
normalize: absent nullable finding metadata becomes null and revision integers
become lossless decimal strings. Matched text, offsets, flags and choices are kept.

The matrix includes all 11 shared review-conflict scenarios: source flags cannot
spoof a channel or native SSN precedence, duplicate origins retain metadata,
containing coverage remains safe, crossing ranges stop handoff, exact not-sensitive
choices remove only the target obligation, and malformed waived findings still
reject before filtering. Additional committed-recall scenarios check Unicode
case/whitespace expansion, Ann versus Annette, exclusion of unapproved fuzzy
matches, no recall before activation, and every overlapping committed identity.
Crossing recall candidates remain visible and prevent preparation across all SDKs.

The reference adapter validates plan access before explicit `discard`, matching
the public opaque SDK lifecycle contract. Core's lower-level cleanup remains
idempotent; implicit/context cleanup is not treated as a second explicit public
operation. Repeated explicit discard and discard after owner close are exercised
in the same fixture across all SDKs.

## Mandatory streaming profile

The same invocation also executes `fixtures/workspace/streaming-contract-v1.json`
against actual Core, installed Python/Node and Chromium WASM. Its 13 cases retain
40 shared commands; each is replayed in both buffered and progressive modes,
producing 80 recorded stream steps. This streaming profile contributes 13 cases and 80 recorded steps to the
complete four-SDK run, plus the shared installed JS conformance assertions. The fixture is not sampled and neither mode is optional.

Each SDK retains `rust-streaming.json`, `python-streaming.json`,
`node-streaming.json` or `wasm-streaming.json` with full chunk text, exact delivered
counts, completion, all issue coordinate systems and error metadata. Buffered
replays preserve R2 literal EOF/report semantics; progressive replays prove late
failure, bare/unknown/damaged markers, encoding splits, EOF/oversized prefixes,
reset, cancellation, closed workspaces and changed saved heads. Strict and
progressive can intentionally differ; each mode must match the same Core reference.

The report retains both fixture hashes and per-profile counts. Missing runtime
methods, missing modes/steps/counts/ranges, partial output attached to failures,
a falsely complete progressive feed or a strict report with text alongside issues
cannot qualify. CLI success requires every named profile and both streaming modes.
This is deterministic streaming parity; it does not certify real server delivery,
distributed admission or overall beta.3 freeze.

## Mandatory operation/history profile

`fixtures/workspace/operation-contract-v1.json` adds seven scenarios and 185 real
steps. Together with foundation and streaming, this profile contributes 37 scenarios /
421 steps across Rust, installed native Python/Node and Chromium WASM. Foundation remains 17/156 and
streaming 13/80. The historical operation-profile receipt records the eight
foundation/streaming outputs that were byte-identical to `d5584e9` at that
qualification. Current candidate bytes also include the conversation-enabled v3
layout; the current receipt below records those exact outputs. No runtime can omit the new profile or substitute
a declaration, mock or skipped method. `cargo dfv parity --workspace` also rejects
a receipt without an executed operation profile.

The shared commands exercise timed creation, Unicode atomic batches, candidate
export and rejected activation, exact canonical saved bundles, reopened records,
full and explicit-window reads, recorded-only counts/unknown prior history,
nullable legacy timestamps, policy rows, explicit outcomes and finite recovery.
Every recognized current/mapping/settings/history-only ID has both legacy and
explicit replay-error checks. Save failures, cancellation/discard, ordinary reads
and strict/report restoration must leave saved bytes unchanged. A no-op activation
preserves sibling plans, while an activated changed head invalidates them. Closed
handles reject operation access.

Independent fixture expectations assert complete numeric totals, null unknown
times, complete ordered pages and terminal cursors, unchanged saved bundles,
input/finding indices and exact byte/codepoint/UTF-16 coordinates after emoji.
The basic strict `restore` returns its existing sanitized error code; the separate
`restoreReport` call supplies the full issue ranges. Neither is normalized into
the other. Records reject extra content fields, missing deltas or invented times
in protocol validation. Raw per-SDK `*-operations.json`, the fixture hash and
package/source receipts remain beside the foundation and streaming results.

The Rust reference still executes actual Core methods for every state change,
count, snapshot and error. Its test-only plan-owner registry mirrors the already
public opaque SDK lifecycle: discard owned siblings only after a successful
activation changes the head; preserve them on same-head/no-op or failed activation.
It does not add that handle-registry policy to the lower-level Core runtime or
replace Core state with expected fixture values.

This profile qualifies deterministic operation semantics and saved-head receipts.
Send confirmation and conversation reservation have their own profile below;
all-device capacity and overall C1/G3 completion remain outside this proof. Actual encrypted capacity and
32-pending-plan memory measurements remain in #162's separate qualification.

[Committed operation-profile receipt](verification/evidence/workspace-operation-parity.json)
records the clean qualified commit, source/package/fixture hashes and canonical
per-runtime hashes. The CLI retains the corresponding full outputs and installed
packages under its run directory; the same command reproduces them without a
single-SDK bypass.

## Separately counted native server profile

[Native-server qualification](native-server-parity-0.5.0-beta.3.md) runs nine real
encrypted-store/process scenarios and 28 checkpoints for Python/Node only. It
retains raw host errors and actual winning candidate IDs against deterministic
Core bundles, plus both existing high-level handoff/stream suites. Its profile
SDKs/counts stay separate from the four-SDK totals. The independently counted
conversation-host profile below supplements these existing CAS scenarios;
shipping-host security approval remains outside both proofs.


## Mandatory conversation profile

`fixtures/workspace/conversation-contract-v1.json` adds eight cases and 179
commands. The complete four-SDK gate compares **45 cases / 600 steps** across
actual Core, installed native Python/Node and Chromium WASM. Every conversation
method and the prepared-change `sendIntents` accessor is mandatory. The reference
calls the same public Core methods as the bindings; it never supplies expected
snapshot bytes or substitutes a compatibility restore for a scoped call.

The cases cover atomic multi-input send preparation, pending intents pausing
whole-context restoration, partial saved confirmations, permanent logical IDs,
one-time site binding, definite non-delivery, reopen/recovery, legacy exact-byte
no-ops and explicit upgrades. Scoped strict, buffered and progressive restoration
retain original Unicode issue ranges and exact delivered counts. Changed heads
invalidate streams; same-head no-op activation preserves them. A failed strict
report can count eligible markers while returning no text: an unissued marker
must not contribute to `restoredMarkers`, and a mixed eligible/unissued response
must not erase the eligible count.

Raw per-SDK `*-conversations.raw.json` and validated `*-conversations.json`
retain exact full heads, canonical inner bundles, actual send-intent IDs, numeric
operation totals, nullable unknown times/history and original errors. Progressive
failures preserve the actual public `complete` flag and prior delivered counts;
validation never replaces those fields with expected constants. JavaScript SDK
heads must already contain canonical decimal-string revisions before any shared
normalization. Each actual Node/WASM run also mutates its captured revisions to
numbers and must reject them; Python alone converts its exact native integers
for the common wire. Mutation tests
reject altered counts, malformed heads, invented ranges, omitted methods and
partial text on failed strict calls.

## Separately counted conversation-host profile

[Conversation-host qualification](native-conversation-host-qualification-0.5.0-beta.3.md)
executes 16 synthetic encrypted SQLite scenarios and 66 checkpoints in actual
installed Python and Node. Its Core oracle uses the same committed conversation
command protocol. The report retains `nativeServer.conversationHost` separately
from the four-SDK 45/600 totals, the existing native CAS 9/28 totals and the
mandatory 16-cycle plan-pressure proof. Existing Python (27 cases) and Node
(17 checks) host suites remain mandatory too.

These are reference-host/process proofs of saved-head, request and confirmation
boundaries. They do not demonstrate a provider send acknowledgement, authorize
retry after ambiguous delivery, or approve a production host's authentication,
key storage, site binding or deployment. Final qualification requires the complete
current profile and raw oracle comparisons; a prior structural-validator pass
cannot substitute for them.

[Clean conversation-profile receipt](verification/evidence/workspace-conversation-parity.json)
retains the qualified commit, actual package/source/output hashes and separate
host applicability. All original 37 scenarios / 421 commands still execute
against the current conversation-enabled state format.
