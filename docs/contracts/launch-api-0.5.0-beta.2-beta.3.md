# Launch API inventory — 0.5.0-beta.2 / 0.5.0-beta.3

Status: **review draft, not the final G3 API or snapshot freeze**. This reconciles
[#143](https://github.com/DataFog/datafog-core/issues/143) with the actual committed
SDK stack and prepares [#154](https://github.com/DataFog/datafog-core/issues/154).
A declaration, merged document or local installed package is not a published SDK
release or proof that the shipping extension/server has passed qualification.

## Release, main and candidate boundaries

| Layer | Verified status | Consumer boundary |
| --- | --- | --- |
| `0.5.0-beta.1` | Earlier released integration baseline | Historical [integration guide](../extension-integration-0.5.0-beta.1.md) |
| `0.5.0-beta.2` | [Published October 29](https://github.com/DataFog/datafog-core/releases/tag/v0.5.0-beta.2), source `2121cbb90be58f2c81d453dfa0b06d40a22c4c52` | Sources, recall, protected choices, combined review, scoped rules and stronger reference-host backup profile; [release guide](../releases/0-5-0-beta-2.mdx) |
| Native workspace/streaming foundations, reference server hosts and four-SDK foundation/stream parity | Merged after the beta.2 release: #174/#175/#176/#177/#178/#179 | Build the pinned checkout; these are not retroactive capabilities of the published beta.2 packages |
| Core/WASM ledger, counts, coverage and paging | #162 merged into main at `699887e6d13164c9549494dd4ba8b0ec32ce8f6f`; awaiting beta.3 publication | Approved history implementation, not completion of all C1/C2 or a joint snapshot freeze |
| Native Python operation adapter | [#186](https://github.com/DataFog/datafog-core/pull/186) merged at `aee4a415`; awaiting beta.3 publication | Same Core contract with native snake_case calls and exact Python values |
| Native Node operation adapter | [#187](https://github.com/DataFog/datafog-core/pull/187) merged at `507b92b0`; awaiting beta.3 publication | Main now contains the combined Core/Python/Node operation surface; compile success still does not freeze/release it |
| Conversation/send-intent scoped restoration | Approved product obligations plus proposal work in #148/#182/#185 | No conversation runtime methods or final key9 layout in this SDK seam; reservation and first-chat logical ID/binding approved; implementation pending |
| CSV and provider qualification | #149 / #150 remain separate decisions/execution gates | No CSV/provider API or provider calls are introduced by this inventory |

The adjacent [type inventory](launch-api-proposal.d.ts) imports actual WASM and
Node declarations; it does not augment `Workspace`, emit code or declare future
conversation methods. [TypeScript examples](launch-api-examples.ts) cover the
shared JavaScript surface and [Python examples](launch-api-python-example.py)
cover the real native names. Read them against this candidate, not an older
registry package. The SDKs remain thin; Core owns normalization, offsets, markers,
recall, policy, protected coverage, snapshots and transaction semantics.

## Shared ownership and lifecycle

The host owns credentials, encryption, durable storage, tenant/request/workspace
authorization, approval, site identity and observation of actual sending. An ID,
clock, source flag or candidate preview grants none of that authority.

Every mutation follows prepare → review/approve → candidate export → encrypt and
atomically compare-and-save with the **complete expected head** → confirm the
saved head → activate. Only then may approved text leave the review surface.
A head includes exact revision plus a 16-byte commit ID; matching revision alone
is insufficient. Operation IDs are fresh host-random 16-byte values, not hashes of
source text. Creation is an unsaved bootstrap until the host persists its bundle.

Reads, scans, recall, review, restore, preview/export, discarded plans and failed
calls/saves do not silently change committed state or append outcomes. An edit,
lock or changed head invalidates approval. Failed save releases no text; uncertain
acknowledgment requires reopening durable state and fresh review, never forced
activation or automatic resend. Successful activation consumes a plan and clears
stale siblings; same-head no-op activation preserves same-head siblings/streams.
Each workspace retains at most 32 live plans. Discard/close request-owned handles;
closing/dropping the parent invalidates its children. Settings, outcome and
recovery plans have zero text previews and confer no handoff permission.

All ranges are zero-based/end-exclusive and refer to the original input. Findings
supply explicit UTF-8 byte ranges matching the exact source slice. Results also
expose codepoint and UTF-16 ranges, with a separate origin per batch member.
JavaScript UI selections use the actual `workspaceByteRangeFromUtf16` helper.
Neither normalization nor batching changes what a returned coordinate means.

## Released 0.5.0-beta.2 behavior

### D4: declared source and review flags

Merged PR #156 adds optional input `source: "scanner" | "recall" | "user"` and
`needsReview: boolean`, and required corresponding transformation fields. Omission
defaults to `scanner` / `true`; explicit null/invalid values reject. These are
caller-declared display fields, not authenticated detector evidence or approval.
`false` still requires the ordinary host approval/save/activation flow. Sources
are not inferred from detector-name strings.

D4 alone preserves today's selection behavior: the retained finding's metadata
follows it; otherwise identical duplicates retain their first input's metadata.
It changes no snapshot or candidate bytes. Existing constructor callers retain
defaults; Rust struct literals need the added fields. Additive output requires
exact-key consumers to refresh validators. The released combined-review API applies the explicit D8 fusion rules below;
a source flag alone never establishes precedence.

### D5: read-only recall of committed values

Released #164 exposes `recall(text): WorkspaceRecallResult` in the JavaScript SDK. It reads only committed entries and
returns findings tagged `recall` / `true`, a head and original coordinate ranges.
It allocates no aliases and changes no state. Callers may feed these findings
into existing preparation; the released `prepareReview` recomputes
recall itself. An empty complete result means no eligible committed match, not
that the input is free of PII.

Approved matching contract:

- Reuse exactly `identity_key`: Unicode **character** lowercase, Unicode whitespace
  runs collapsed to one ASCII space, leading/trailing whitespace removed. No NFC,
  accent stripping, full case-folding, punctuation removal or fuzzy matching.
  `ß` does not become `ss`; `İ` expands to `i` plus combining dot. An expansion must
  map back to whole original scalars; never return half of an expanded character.
- Preserve a mapping from normalized units to complete original byte spans.
  Confirm that normalizing each original matched slice equals the stored key.
  Internal whitespace variants belong in the returned span; surrounding whitespace
  does not. Restoration keeps the first committed spelling.
- At each edge that is a word character in the key, require an original-input
  word boundary. Word characters are Unicode Alphabetic,
  Mark, Decimal_Number, Connector_Punctuation, U+200C and U+200D. This prevents
  `Ann` in `Annette`, `_Ann` and a prefix before a combining mark. Punctuation
  inside values stays significant; edge punctuation is not stripped. This is a
  Unicode category boundary rule, not locale-specific linguistic segmentation.
- Skip valid committed marker spans. Unknown/damaged/bare committed markers fail
  the same source preflight as preparation, without mutating state. Do not recall
  inside markers or from staged/discarded/unactivated values.
- Return **all** valid overlapping candidates before D8 selection, ordered by
  original start, end and entity type. Deduplicate identical type/range matches;
  never silently drop a smaller protected candidate. Two types for the same
  spelling stay separate. Every prepare path must enforce the approved protected-coverage guard; D8
  additionally combines the three source channels.

The approved recall ceilings are 1 MiB source, 20,000 stored entries,
10,000 returned findings, 4 MiB normalized source, 16 MiB normalized keys,
262,144 trie nodes and 32 million charged work units per recall. Batch preparation
retains its 32-input and aggregate source/finding limits. Normalization, matching
and candidate emission count toward budgets. Exceeding any cap rejects the whole
call with `resource_limit`: no partial success, skipped eligible entries or silent
truncation. #164 records browser measurements and cleanup evidence; these bounds
are resource safeguards, not a universal latency or low-memory-device guarantee.
Policy filtering happens before indexing, so only eligible committed identities
count toward index/matching budgets.

### D6/D7: type policy and remembered choices

**Approved by Sid on October 28:** all types default to `hide`; protected types
forbid generic `keep`, including per-value keep. One explicit `not_sensitive`
false-positive correction may leave the exact existing normalized `(entity type,
value)` visible. Core cannot verify its truth, so a person may intentionally
expose actual PII with this exception. A source/needs-review flag or detector name
never grants it. A narrower or differently typed correction does not waive wider
protected coverage. [ADR 004](../adr/004-workspace-aliasing-v1.md) records this
exception; the prior blanket rejection of `not_sensitive` is superseded.

Merged #166 protects all built-in PII types plus `PERSON`, `FIRST_NAME`,
`LAST_NAME`, `ADDRESS`, `STREET_ADDRESS`, `ACCOUNT_NUMBER`, `CUSTOMER_ID`,
`IDENTIFICATION_NUMBER`, `URL` and `USERNAME`. Unknown valid custom types still
default to hide and may use generic keep. The shipping inventory must track the
Core registry; `ADDRESS` and `STREET_ADDRESS` stay distinct.

The released JavaScript API exposes read-only `policy()` and transactional
`preparePolicyChanges({rules, choices}, operationId)`. Rule edits use
`{entityType, rule: "hide" | "keep" | null}`; choice edits use
`{entityType, value, disposition: "keep" | "not_sensitive" | null}`. Null removes
that setting. Choices may be saved before any alias exists, without allocating
one. Update/removal requires a new reviewed transaction. Older aliases, first
spellings and counters remain permanent. Exact no-ops preserve head, bytes and
snapshot version.

Validate every supplied finding before filtering policy-eligible identities in
**all** single/batch/combined preparation paths and recall. An invalid suppressed
finding must still reject. Limit to 10,000 edits per call, 20,000 rules and 20,000
choices, 1 MiB per choice value and 16 MiB aggregate state. Reject duplicates,
invalid labels, protected keep, malformed dispositions or excess bounds atomically
with `invalid_policy` or the applicable existing validation/resource error.
Settings plans have a before/after policy preview and no text previews; they do
not authorize handoff. Exposed choice display values belong only in the unlocked
trusted review surface, never telemetry/errors.

### D8: one Core-owned review list

Merged [#173](https://github.com/DataFog/datafog-core/pull/173) exposes `review(input)` as read-only and
`prepareReview(inputs, operationId)` as the authoritative transaction in 0.5.0-beta.2. The complete beta.3 launch
contract is still provisional. Both validate
scanner inputs and user selections, compute committed recall and apply policy.
The latter recomputes against the current head and source rather than trusting a
previous review list. Caller-supplied `source` does not grant precedence: the
separate scanner/user input channels and explicit correction command establish
the declared intent, all still subject to Core validation and protected coverage.

The shared implementation enforces these approved protection requirements:

1. Apply permitted choices to their exact type/value identities. Record remaining
   protected intervals as coverage obligations before resolving overlaps.
2. Exact same type/range duplicates become one candidate; retain a sorted unique
   `sources` list in review results. Choose display source
   `user > recall > scanner`, with stable detector metadata tie-breaks. This
   changes D4's first-input tie only in the new combined-review API. Combine
   `needsReview` with logical OR across retained duplicate contributions; one
   caller's false flag cannot clear another contribution's review request.
3. Prefer the precise native SSN attribution over PERSON on exactly equal spans
   as implemented in the focused [#155](https://github.com/DataFog/datafog-core/pull/155). Do not
   generalize that change into global protected-type ranking or let a source
   label pretend to be native detector evidence. Native SSN attribution requires
   actual Core SSN corroboration. Return `type_conflict` for
   unresolved equal-span type conflicts rather than invent an ordering.
4. A containing retained span may cover nested candidates only when it replaces
   their full protected text. A manual correction cannot shrink/drop an existing
   protected interval or relabel it into an allowed disclosure. Validate its
   exact source slice and review-local target key; stale keys reject.
5. For partial overlaps whose selection would expose any protected suffix/prefix,
   return ranged `protected_coverage` conflicts; preparation
   rejects with `review_conflict`. Require a fresh covering selection/correction
   and approval. Do not silently choose one finding or introduce a synthetic
   composite entity/marker format in this slice.

`review({text, scannerFindings, userSelections, corrections?})` returns
`{head, candidates, conflicts}`. A correction is `{candidateKey, replacement}`.
Each candidate has `key`, `finding`, `sources`, all three original ranges,
`eligible` (policy requires hiding) and `selected`. Every candidate is unselected
while any conflict remains. Conflicts carry `kind: "protected_coverage" |
"type_conflict"`, `findingIndex` and original byte/codepoint/UTF-16 ranges;
they do not contain candidate-key lists or source text. Conflict `findingIndex` retains original scanner, user then recall indexing
through grouping and corrections. Optional confidence/detector metadata can
be absent or explicitly null, as the actual declarations specify.

Review keys identify the uncorrected candidates before applying corrections; they
are ephemeral, deterministic for the same head/input and detection data, and
contain no raw text. Do not persist or use them as authority. Legacy supplied-finding `prepare`/batch calls remain available without automatic
recall, but must validate all findings, apply the same exact-identity policy and
stop before staging any plan when selection would lose protected coverage. The
shared guard (#172) and #146 cover these old and new paths; a host cannot bypass
the rule by choosing an older method. Conflicts produce no candidate text or
state mutation. Only the new combined-review path computes recall automatically. UI highlights use returned UTF-16 ranges; Core owns selection.

## Implemented C1/C3 on main

The merged Core/WASM and native call names are `createWithOperation`,
`prepareWithOperation`, `prepareBatchWithOperation`,
`prepareRecoveryWithOperation`, `prepareOutcome`, `operations`, `operationsPage`,
`counts` and `historyCoverage` (Rust/Python use snake_case). These are real methods
in the cited source layers, not signatures supplied by the docs-only artifact.
`prepareReview` remains the implemented combined-review entry point; no timed
`prepareReviewWithOperation` method is invented here. Supplied-finding preparation
does not silently acquire automatic recall.

An operation context is `{id, occurredAtMs}`: host-random 16-byte ID and untrusted
integer UTC Unix milliseconds in `0..253402300799999`. Clocks may go backwards;
head revision orders commits. Legacy ID-only calls retain their signatures and
record unknown time (`null` / Python `None`) on actual saved mutations. Old absent
history is not reconstructed. Known reused IDs reject with `invalid_operation_id`
on legacy methods and `operation_reused` on explicit-context methods. Neither
category permits reuse or implies exactly-once external delivery.

Each retained row contains `id`, `kind`, nullable `occurredAtMs`, `preparedInputs`
and `transformedSpans`. Current kinds are `create`, `prepare`, `recovery`,
`policy_change`, `copy_confirmed`, `user_cancelled`, `blocked_input` and
`restore_blocked`. There is no implemented `handoff_confirmed` kind in this seam.
No source text, normalized identity, arbitrary message, URL or issue details
belong in these rows. Content-free metadata is still private vault data.

| Counter | Actual recorded-history meaning |
| --- | --- |
| `operations` | Every retained committed row, including create/recovery and explicit outcomes |
| `preparedInputs` | Sum of recorded preparation input members, including empty findings |
| `transformedSpans` | Replaced source occurrences, including repeated values, not distinct identities |
| `confirmedHandoffs` | Reserved at zero until the reviewed observed-send transaction exists |
| `reportedBlockedAttempts` | Explicitly saved blocked-input/restore rows, one per attempt rather than per issue |

An explicit preparation records success even when no mapping changes. Legacy
no-ops and exact settings no-ops retain their bytes/head/version. A batch records
one row with its member/occurrence deltas. Real policy saves record a zero-delta
`policy_change` row; legacy times remain unknown. Counts are recomputed from row
deltas during strict decoding. Preview, candidate export or failed activation
never increments committed totals.

`prepareOutcome` accepts only copy/cancel/blocked/restore-blocked assertions.
Copy/cancel require `issueCount: 0`; blocked outcomes require 1–10,000. Core validates
that count but does not retain it or its content. Copy confirmation is not proof
of sending, does not establish conversation membership and does not increment
`confirmedHandoffs`. Observing an error never implicitly writes an outcome.

`historyCoverage()` returns `{recordedHistoryOnly: true, preUpgradeHistory}`.
The latter is `"not_applicable"` when retained history starts at its create row,
otherwise `"unknown"`. Empty v1/v2 history and unknown legacy activity are never
presented as known lifetime totals.

### Retention, explicit pages and finite recovery

Sid approved a provisional 100,000 retained-row ceiling with the final 256 slots
reserved for recovery. New ordinary records stop at 99,744 rows. At 100,000,
further recovery also fails explicitly; reads/export remain available. No row,
old ID or covered total is evicted, sampled or truncated. Postlaunch lossless
compression remains #165. This bound is not unlimited history or a low-memory
platform guarantee.

`operations()` continues to return all rows. `operationsPage(offset, limit)` is an
additive read-only window with `{records, total, offset, nextOffset}`. Limits are
explicitly 1–100,000; offset may equal total (empty final page). Invalid bounds
reject with `resource_limit`. A null cursor means the end, not dropped rows.
Concatenating pages over an unchanged head equals the full view. Result mutation
cannot change saved state. The host may use pages to avoid a full JSON allocation;
there is no hidden pagination or changed full-history default.

The v3 encoder reserves 7,699 bytes for 256 maximum supported recovery rows plus
conservative head/array/count header growth. Ordinary records use the reduced
budget; recovery may use the full 16 MiB cap. Valid old vaults already near that
cap remain readable/exportable and can hit the byte ceiling earlier. No data is
trimmed to make room. Final conversation payload/recovery growth must be reconciled
before freezing a joint v3 format; these recovery-only slots are not silently
assigned to send confirmation. See [operation evidence and limits](../proposals/workspace-operation-records.md).

## Native names, exact values and typed failures

| Contract | JavaScript WASM / native Node | Native Python | Rust |
| --- | --- | --- | --- |
| Loading | WASM `await init()`; native Node needs no browser init | Installed compiled package | Direct Core |
| IDs / operation context | `Uint8Array(16)` / `{id, occurredAtMs}` | `bytes` / exact dict with the same camelCase fields | `PublicId` / `OperationContext` |
| Complete head | `{revision: decimal string, commitId: number[]}` | Immutable `WorkspaceHead(revision: int, commit_id: bytes)`; nested result dictionaries use camelCase and exact ints | `Head {revision: u64, commit_id}` |
| Plan heads | `plan.heads()` | `expected_head()` / `candidate_head()` | Same two methods |
| Prepare/review/policy/recovery/activate | camelCase actual declarations | snake_case actual stubs, typed owned input classes | Typed Core values |
| Operations/pages/counts/coverage/outcomes | Actual names above | `operations_page`, `history_coverage`, `prepare_outcome`, other snake_case counterparts | Borrowed Core row/page views |
| Results | Owned public arrays/objects/bytes | Owned dict/list values and exact bytes, copied from Core | Borrowed/owned values as declared |
| Errors | `DataFogError` with typed `code`, optional/nullable indices, `markerIssues`, `reviewConflicts` | `DataFogWorkspaceError` with `code`, optional snake_case indices and camelCase ranged dictionaries | `WorkspaceError` / `ErrorCode` |

Core failures use null for absent indices; JavaScript transport failures may omit
them (undefined). Python indices are int or None.

Actual workspace codes include `closed_workspace`, `inactive_plan`,
`inactive_stream`, `workspace_conflict`, `invalid_finding`, `invalid_encoding`,
`review_conflict`, `invalid_batch`, `invalid_marker`, `invalid_snapshot`,
`invalid_operation_id`, `invalid_policy`, `resource_limit`, `invalid_operation`
and `operation_reused`. Error messages/codes and safe ranges contain no source or
restored values. Errors do not become successful empty results. Typed declarations
do not prove random IDs, numeric bounds, authorization, storage durability or plan
ownership; Core and the trusted host still enforce them at runtime.

## Implemented strict and opt-in progressive restoration

The approved #169 contract is implemented by merged #176/#174/#175. Strict
`bufferedRestore` / Python `buffered_restore` is the default response adapter:
feeds return no text, and finalization returns the existing all-or-nothing R2
report. Invalid marker reports contain null text; call-level failures are typed
errors. Strict buffering and nonstreaming R2 retain their existing literal EOF
behavior. Never convert a null report to an apparently successful empty response.

`progressiveRestore` / `progressive_restore` explicitly opts into irreversible
chunks. A successful feed returns `{text, delivered, complete}`; completion is
true only after successful finalization. A later marker/encoding/resource failure
is terminal/incomplete, returns none of that failing call and cannot retract
earlier successful output. Potential marker prefixes unfinished at EOF or exceeding
the supported bound stop progressive restoration instead of being flushed as
literal text, including the approved false-positive tradeoff for code/literal
syntax. This stricter prefix rule does not change strict/default R2 behavior.

Delivered counters are `bytes`, `codepoints`, `utf16` and `restoredMarkers` from
prior successful Core calls. They describe text returned to the caller, not proof
of network delivery. JavaScript terminal errors carry `delivered` and
`complete: false`; Python uses `DataFogWorkspaceStreamError`, extending the shared
workspace error with the same values. Issue ranges remain original cumulative
input coordinates. No raw failed-call text or provider error message is returned.

UTF-8 feeds use `Uint8Array` / Python `bytes`; UTF-16 feeds use `Uint16Array` /
lists of exact uint16 integers. Core holds split encoding/marker carry and enforces
bounds. Both adapters expose finalize/reset/close; reset does not retract earlier
output. Closing/dropping the parent or activating a changed head clears children;
failed saves and same-head no-ops preserve them. Hosts own provider iteration,
cooperative cancellation, output callbacks and incomplete-status presentation.
Stop on host/provider failure too; do not automatically retry, resend or fabricate
completion. Provider SSE/tool parsing and conversation-pinned streaming are not
implemented by these examples.

## Snapshot, backup and host boundary

Released policy-v2 uses keys 0–5 in their established roles, policy at key6 and
choices at key7. Actual v1/v2 bundles read and re-export byte for byte; reads,
no-ops, discarded plans and failed saves do not upgrade them. Released beta.2
new workspaces use v2. Beta.1 cannot open policy-v2 state; warn before saving a
real v1→v2 upgrade and preserve old backup files.

The operation candidate uses a distinct v3 map: version/key0 and established
fields through7, operation rows at8, counts at10, with key9 **absent/reserved** for
joint conversation work. Key5 remains an empty reserved merge slot. New candidate
workspaces bootstrap v3; a real approved saved legacy mutation/recovery stages v3.
Warn **before saving** that beta.1 and beta.2 cannot open the upgraded v3 vault.
Keeping an old backup does not make later state readable by its old decoder.
The old unshipped operation-v2 layout is rejected, not mistaken for policy-v2.

This is not a frozen joint v3: #148 still needs coordinated key9 storage,
reservation/confirmation accounting, canonical malformed-decoder tests, encrypted
reopen/recovery, complete field backup coverage and four-SDK comparison. Unknown
fields are not silently dropped. No fake contract-version export is introduced.

The reference-host backup flow reauthenticates a freshly entered password for
every export, even while unlocked. Argon2id is the approved 256 MiB / t=3 / p=1
strict v2 export profile, with explicit legacy imports and no weaker inner
password wrapper. Imported backups follow the approved profile/version and fresh
destination recovery-head/key rules. A forgotten-password/recovery-only user does
not gain a reset/export bypass. Credentials/keys never enter workspace snapshots,
metadata or errors. These are host obligations, not a `Workspace.exportBackup`
SDK method. See [backup/KDF decision #142](https://github.com/DataFog/datafog-core/issues/142).

Merged Python/Node server examples qualify a bounded synthetic topology with
explicit approval, authenticated key lookup/AAD and durable complete-head CAS.
Exactly-one-winner claims apply to state-changing candidates; same-head no-ops do
not deduplicate delivery. Core's 32-plan bound is per workspace handle; host-wide
admission and cross-worker coordination remain host duties. Uncertain saves need
reopen/fresh review. Client-carried snapshots need authenticated anti-replay policy;
resent chat text alone cannot reconstruct mappings. G4 #153 reviews actual shipping
hosts/keys/storage; a reference harness is not certification of Josh's extension
or a production server. No LiteLLM/provider adapter is supplied here.

## Approved conversation intent; unresolved implementation

[Issue #148](https://github.com/DataFog/datafog-core/issues/148) records two explicit
October29 approvals, replacing the older insertion-based sketch:

- Mappings save/activate before external handoff. Only the trusted host's observed
  successful **send to that exact chat** establishes issuance through a second
  prepared/save/activate transaction. Preview, copying, composer fill and insertion
  do not count as successful sending. Host assertions are not Core proof of remote receipt.
- Save a content-free pending-send intent in the first approved mapping transaction.
  It is intention, not issuance. While any intent remains unresolved, **all scoped
  restoration for that conversation pauses**, including older replies and ordinary
  text. Other conversations remain usable. Crashes, backup/reopen and failed/uncertain
  confirmation saves preserve the pause. Reconcile the exact confirmation without
  inventing success or automatically resending.

- Before sending, reserve the confirmation row and encoded-byte space. Other
  activity, including recovery, cannot spend those credits. The approved 100,000
  retained-row ceiling and 256 recovery-only slots remain unchanged. This is the
  approved reservation requirement, not an implemented API or final wire allocation.

- A first chat has a private permanent logical ID before its site ID exists,
  then one verified binding to the actual observed site conversation. The host
  must verify that binding; ambiguity stays unresolved. Never guess a chat ID or
  automatically resend to manufacture confirmation.

The explicit unscoped compatibility API remains unchanged; the host must never
silently fall back to it to bypass the pause. Identity acquisition/validation,
confirmed membership, safe wrong-context ranges, joint key9 layout and actual host
send observations still require implementation and review. The approved logical-ID/binding and reservation requirements need coordinated
implementation and actual host verification/proof; it does not make
failed/uncertain durable saves successful or allocate recovery-only slots to sending. This
inventory declares no conversation/reservation method signatures or manual-confirmation
escape hatch. The current SDK seam has none of those runtime calls.

CSV #149 scope and provider/model/sampling/denominator/execution choices under
#150 remain pending. [G1 preparation #184](https://github.com/DataFog/datafog-core/pull/184)
is an offline synthetic packet with zero provider runs and no preservation rate
or G1 pass. Its gold/witness must be deliberately regenerated against the coordinated
v3 source before future runs; no provider calls or CSV implementation follow from
compiling this document.

## Remaining launch gates and compilation

| Requirement | Implementation / remaining evidence |
| --- | --- |
| Released sources/recall/policy/combined review/rules/stronger backup | beta.2 release; #140/#141/#144/#145/#146/#142 |
| C1/C3 recorded operations | #147/#162; actual native adapters #186 and Node operation stack; shared operation parity remains |
| C2/R3 conversation/send intent/restore | #148/#182/#185; approved sequencing/pause/reservation/logical-ID binding; implementation and final format pending |
| X6 / X7 native bindings | #167/#168; installed wheel/package and typed errors, plus future conversation integration |
| R4 strict/progressive | #169/#176/#174/#175 implemented; native/host/provenance acceptance still joint work |
| X8 workspace parity | #171/#179 foundation+streaming actual four-SDK matrix; operation profile #188 under review, conversation profile still needed |
| W6 / G4 server and vault | #170/#153; actual host ownership, keys/storage/admission/cancellation and shipping review |
| B2 complete backup | #152/#181; extend field-to-fixture coverage for every newly committed joint-v3 field |
| G1 / G2 qualification | #150 real provider trials; #151 actual shipping model/rules/recall evidence |
| D9 CSV | #149 if approved; no signature or implementation here |
| G3 freeze | #154 only after full actual API/decoders/backup/native/parity/host qualification; this document does not close it |

TypeScript uses the pinned compiler without emitting or running these examples:

```sh
npm ci --prefix bindings/wasm
bindings/wasm/node_modules/.bin/tsc --project docs/contracts/tsconfig.json
```

Python examples type-check against the checkout's actual stubs in CI and an
actual installed native wheel during local qualification:

```sh
python -m pip install -r bindings/python/tests/requirements-typing.txt
MYPYPATH=bindings/python/python python -m mypy --strict --no-incremental --python-version 3.10 docs/contracts/launch-api-python-example.py
python -m pyright --project docs/contracts/pyrightconfig.json
```

Negative assertions reject unsupported calls/types; they do not promise future
method names. Compilation proves declaration consistency only. Runtime correctness,
randomness, privacy, crypto, delivery, approval and durable storage still require
Core tests, installed SDKs, original-coordinate checks, actual encrypted browser
and server flows and exact source/package/model identities. Preserve all reported
failures instead of skipping unsupported runtimes or producing empty success.
