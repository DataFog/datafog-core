# Launch API review draft — 0.5.0-beta.2 / 0.5.0-beta.3

Status: **review draft, not a frozen launch contract**. This is the X2 review for
[#143](https://github.com/DataFog/datafog-core/issues/143). Approved decisions below
are distinguished from runtime candidates and future proposals. A merged review
document does not ship an SDK capability. Synthetic data remains the integration
baseline until the pilot gates pass.

## Supported runtime versus proposals

| Layer | Status | Consumer entry point |
|---|---|---|
| `0.5.0-beta.1` detection/model and workspace lifecycle | Released | `@datafog/wasm`, `@datafog/wasm/models`; [integration guide](../extension-integration-0.5.0-beta.1.md) |
| D4 sources/review flags, D5 recall, D6/D7 policy and B3 backup profile | [#156](https://github.com/DataFog/datafog-core/pull/156), [#157](https://github.com/DataFog/datafog-core/pull/157), [#164](https://github.com/DataFog/datafog-core/pull/164) and [#166](https://github.com/DataFog/datafog-core/pull/166) merged into main; not yet released as beta.2 | Current checkout declarations and reference host; beta.1 packages do not gain these changes |
| D8 combined review | [#173](https://github.com/DataFog/datafog-core/pull/173) merged into main, targeting beta.2 | Actual checkout declarations; not yet released as beta.2 |
| C1–C3/R3, native SDKs, streaming and server qualification | beta.3 scope; design/implementation pending | No future runtime exports are supplied by this document |

The adjacent [declarations](launch-api-proposal.d.ts) are a **type-only review
artifact outside the npm package**. They import actual main workspace types and keep only future beta.3
provenance/conversation proposals separate; neither augments `Workspace` or
manufactures runtime methods. Use the corresponding pinned, verified build for runtime capabilities. Implementation issues ship actual exports,
declarations and versioned guidance together. CSV remains an unresolved scope
decision in [#149](https://github.com/DataFog/datafog-core/issues/149); no CSV API
is proposed here.

## Ownership and unchanged lifecycle

Core owns normalization, offsets, marker validation, source propagation, recall,
policy enforcement, selection, transaction heads and snapshot decoding. Rust and
WASM expose the same semantics; native Python and Node workspace bindings target
beta.3 (#167/#168), rather than being available today; JavaScript does transport validation and host
integration. The extension owns credentials, encryption, IndexedDB transactions,
user approval, site identity and actual insertion/send observation.

All coordinates are zero-based/end-exclusive. Inputs supply explicit UTF-8 byte
ranges matching the exact source slice. Read results expose byte, codepoint and
UTF-16 ranges relative to the original input, never normalized text. Each batch
member has its own coordinate origin. Host selections convert UTF-16 with the
existing `workspaceByteRangeFromUtf16` helper.

Every mutation uses prepare → preview/approve → export candidate → encrypt and
atomically save with the complete expected head → read/confirm saved head →
`activateSaved`. The head includes decimal-string revision and 16-byte commit ID;
matching only revision is insufficient. Operation IDs are fresh host-random
16-byte IDs, unrelated to source text. A preview, read, scan, recall, failed call
or discard never changes committed mappings, counters, records or head.

The host approves the exact current text, findings, batch and generation. An edit,
lock or changed head invalidates that approval. Failed save releases no text;
uncertain acknowledgment requires reopening durable state and fresh review,
not force-activation. Successful activation consumes the plan and reclaims plans
based on older heads. Keep the 32-live-plan limit, explicit discard/close, marker
preflight and atomic strict restore. Policy/record/recovery plans have no text
previews and confer no permission to hand off text.

## Beta.2 approved behavior and implemented main APIs

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
exact-key consumers to refresh validators. The D8 fusion proposal below is a
separate behavior change; approving D4 does not approve D8 precedence.

### D5: read-only recall of committed values

Merged #164 exposes `recall(text): WorkspaceRecallResult` on main. It reads only committed entries and
returns findings tagged `recall` / `true`, a head and original coordinate ranges.
It allocates no aliases and changes no state. Callers may feed these findings
into existing preparation; main's `prepareReview` recomputes
recall itself. An empty complete result means no eligible committed match, not
that the input is free of PII.

Approved matching contract implemented by the candidate:

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

The approved main ceilings are 1 MiB source, 20,000 stored entries,
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

Main now exposes read-only `policy()` and transactional
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
`prepareReview(inputs, operationId)` as the authoritative transaction on main. These APIs await the beta.2 release;
merging them does not freeze the complete launch contract. Both validate
scanner inputs and user selections, compute committed recall and apply policy.
The latter recomputes against the current head and source rather than trusting a
previous review list. Caller-supplied `source` does not grant precedence: the
separate scanner/user input channels and explicit correction command establish
the declared intent, all still subject to Core validation and protected coverage.

The main implementation enforces these approved protection requirements:

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
be absent or explicitly null, as the actual main declarations specify.

Review keys identify the uncorrected candidates before applying corrections; they
are ephemeral, deterministic for the same head/input and detection data, and
contain no raw text. Do not persist or use them as authority. Legacy supplied-finding `prepare`/batch calls remain available without automatic
recall, but must validate all findings, apply the same exact-identity policy and
stop before staging any plan when selection would lose protected coverage. The
shared guard (#172) and #146 cover these old and new paths; a host cannot bypass
the rule by choosing an older method. Conflicts produce no candidate text or
state mutation. Only the new combined-review path computes recall automatically. UI highlights use returned UTF-16 ranges; Core owns selection.

## Decisions recommended for beta.3

### C1/C3: explicit operations and content-free counts

Recommend `OperationContextDraft {id, occurredAtMs}` on new transactional calls.
The timestamp is an integer UTC Unix millisecond in `0..253402300799999`, supplied
by the host and never used as authorization or total ordering. Head revision
orders commits. Existing beta.1 operation-ID signatures remain valid; adding
required time to them would be a breaking change and is not proposed here.
When the future runtime serves a legacy mutation call, record `occurredAtMs: null`
and the actual kind, including create/recovery; never fabricate historical time.
This preserves the call shape, not the ability of an older decoder to open a
new snapshot. Historical operations absent from a development snapshot are not
invented or counted; its compatibility policy must be explicit at the freeze.

Persist operation ID, kind and time in key 8, counts in key 10. New APIs reject an
ID already present in committed records with `operation_reused`; apply that rule
to legacy mutation calls on the future runtime too. This stricter reuse error
needs compatibility review. An unsaved retry
may reuse its still-pending plan. After an uncertain save, reopen and inspect the
committed head/record before deciding on a new operation. Do not invent exactly-once
external delivery from a local operation ID.

**Approved retention direction:** preserve every committed operation record; no
silent eviction, truncation or automatic compaction. The earlier proposed
10,000-record lifetime stop is not an approved capacity contract. #147/#162 must
qualify a coherent bounded representation and explicit failure behavior without
claiming unlimited storage. Lossless history compression is post-launch
[#165](https://github.com/DataFog/datafog-core/issues/165), outside beta.2/beta.3.
Failed, blocked, cancelled, empty read-only and restored calls do not silently
append operations. A host wanting a cancellation/block record must explicitly
stage `prepareOutcome`, save and activate it. Losing that optional record never
turns failure into success.

Recommended counts are persisted aggregates of committed records, not activity
telemetry: `operations` increments once per committed operation; `preparedInputs`
counts input members of successful preparation records (including empty findings);
`transformedSpans` counts replaced occurrences, not distinct identities;
`confirmedHandoffs` counts committed host-observed insert/send records, including
repeated handoffs; `reportedBlockedAttempts` counts explicit blocked outcome
records. Reads, previews, retries of a pending plan and discarded candidates add
zero. Main treats an exact policy no-op as no mutation. Future operation
accounting must preserve that behavior; an explicit outcome record is a separate
transaction, not an implicit policy-no-op record. Stage counters and records atomically in the candidate;
failed save/activation never increments committed counts. No successful-restore
counter is claimed by these types; add a reviewed explicit record if one is needed.

Free-form content, snippets, normalized identities, URLs and arbitrary diagnostic
strings are forbidden in records/counts. Only the documented opaque IDs, bounded
kind enums, timestamps and aggregate numbers are stored. Host-reported outcomes
are host assertions, not Core verification of site activity.

### C2/R3: conversations and actual handoff

Recommend a workspace-local identity `{surface: "chatgpt", siteConversationId}`,
obtained by the host from the actual site. For the initial ChatGPT surface,
propose a canonical lowercase UUID-form ID, at most 36 ASCII bytes, validated by
Core; verify the host's observed format before implementation. Do not use a
tab ID, entire URL, title, text-derived hash or unverified draft identifier. An
unknown/new-chat identity cannot create a conversation record. Other surfaces
need explicit versioned validation, not an arbitrary free-text field.

Save mappings before any text leaves the review surface. **Then**, after the host
observes successful insertion/send and knows the stable conversation ID, call
`prepareHandoffRecord({conversation, action, releasedText}, operation)`, save and
activate that second transaction. Core validates the actual released text with
strict marker parsing, derives the exact committed `(label, code)` set and
discards the text. Key 9 stores conversation ID, creation operation and the union
of markers handed off there, including previously issued markers used in follow-ups.
No raw conversation/history is stored. Proposed bounds: 1,000 conversations and
20,000 unique marker references per conversation, within the 16 MiB total bundle.

An insert acknowledgment is evidence of insertion, not proof the provider accepted
a message. `action` records the observed kind. Clipboard success alone produces
an optional `copy_confirmed` operation and **no** conversation marker membership;
preview, approval, mapping save, failed insert or cancellation likewise do not
claim a conversation handoff. Repeated successful attempts use distinct operation
IDs; membership is a set, while handoff counts reflect recorded attempts.

There is no atomic transaction across site delivery and local encrypted storage.
A crash/failure after delivery but before record save leaves an unrecorded
handoff; do not pre-record delivery to hide that gap. Conversation-aware restore
fails closed for that gap. A recovery/reconciliation UX needs a separate reviewed
decision; this proposal supplies no automatic membership repair or proof of
remote receipt. Record this tradeoff with Josh before implementing #148.

Recommend explicit `restoreInConversation(text, conversation)` returning an
all-or-nothing report. It applies normal unknown/damaged/unbracketed handling and
adds `wrong_conversation` for a known workspace marker absent from that
conversation. Every issue has input byte/codepoint/UTF-16 ranges and no real value.
Any issue means `text: null`. Invalid/unknown conversation IDs reject with
`invalid_conversation`, even for plain text. Keep `restore`/`restoreReport` unchanged
as the explicit compatibility mode; never silently fall back to it after a
conversation check fails. Copies and legacy snapshots with no conversation
records can use compatibility mode only as an explicit host/product choice.

### Snapshot, backup and freeze

**Approved compatibility direction, merged #166:** existing v1 workspaces open
and re-export byte-for-byte unchanged. Reads, exact no-ops, discard and failed
save do not upgrade them. A real saved mutation or recovery stages policy-v2;
new workspaces bootstrap v2. The host must explain **before saving the upgrade**
that beta.1 cannot open the upgraded vault. Existing backup files stay unchanged;
retaining a pre-upgrade backup does not make later v2 state readable by beta.1.
The merged reference host includes this warning in ordinary and recovery flows.

Policy-v2 is the authoritative main implementation targeting beta.2: keys 0–5 retain their roles, key
6 replaces the old `synthetic_v1` placeholder with validated policy, and key 7
stores remembered choices. D4 metadata stays transient; recall adds no snapshot
key. Strict canonical decoding and encrypted reopen/recovery tests are required.
Unapproved operation-record proposal #162 currently uses a different v2 layout;
it **must integrate the merged policy normally and choose a coherent subsequent
version**, never ship different
formats under the same version. Keys 8/9/10 below are future design allocations,
not fields already present in policy-v2. Key 5 stays reserved for merges.

[#154](https://github.com/DataFog/datafog-core/issues/154) freezes the complete
Rust/WASM/native Python/native Node contract and supported snapshot versions only
after parity, native packages, streaming and server-host qualification. Each new
field needs strict malformed-decoder, encrypted reopen and full backup tests.
No silent unknown-field dropping or fabricated historical defaults. This draft
exports no fake contract-version constant.

Merged #157's reference-host backup flow requires **fresh password entry and verification for
every export**, even from an unlocked vault. `freshPassword` is an input obligation,
not a type-level proof of freshness; the trusted host must reauthenticate before
export. The recorded [#142 decision](https://github.com/DataFog/datafog-core/issues/142)
is Argon2id 256 MiB / t=3 / p=1, v2 strict export
profile, no weaker inner password wrapper, with explicit legacy imports. Do not
silently downgrade a profile to handle low-memory devices; fail explicitly.
The adjacent `BackupHostDraft` describes a host obligation, not a WASM export.
An unlocked recovery-only user without the password receives a clear
credential-required failure; no forgotten-password reset/export bypass is implied.
Imported backups need the approved profile/version policy and fresh destination
recovery head/key flow. Never log credentials or keep them in workspace snapshots.

## Main calls, future calls and failure behavior

Main workspace call shapes come directly from the checkout's declarations.
Only future beta.3 proposals use separate docs-only interfaces. Future error names remain proposals.

| Proposed call | Ownership and result | Failure/compatibility decision |
|---|---|---|
| Main `recall(text)` | Core read; head + complete original-coordinate candidates | Existing `invalid_marker`/`invalid_finding`/`resource_limit`; no partial results |
| Main `review(input)` | Core read; candidates, attribution and ranged conflicts | Invalid input rejects; resolvable overlaps appear as conflicts |
| Main `policy()`; future `operations()`, `counts()`, `conversations()` | Trusted local reads of committed state only | Closed workspace rejects; lists obey saved-state bounds |
| Main `prepareReview(inputs, operationId)` | Core mutation plan; validates and recomputes the approved source | `review_conflict`, existing range/head/resource errors; no trusting a stale review |
| Main `preparePolicyChanges(edits, operationId)` | Core settings plan; policy before/after preview, zero text previews | `invalid_policy`; protected generic keep and duplicate edits reject atomically |
| `prepareOutcome(outcome, operation)` | Explicit Core record plan; zero previews | Host-reported event only; errors never append an implicit event |
| `prepareHandoffRecord(handoff, operation)` | Core record plan after actual observed handoff; zero previews | Proposed `invalid_conversation`; invalid markers/limits reject the whole record |
| `restoreInConversation(text, conversation)` | Core read; null text on any issue | Proposed `wrong_conversation` issue; proposed `invalid_conversation` call error |
| `activateSaved(change, expected, saved)` | Existing host-confirmed Core activation | Preserve full-head comparisons, runtime ownership and stale-plan rules |
| `BackupHostDraft.exportBackup(request)` | Trusted host, explicit fresh password verification | Existing host cancellation/auth/resource errors; no weakened fallback |

New operation contexts reject malformed IDs/time with proposed `invalid_operation`
and committed reuse with `operation_reused`. Error payloads stay content-free;
optional finding/input indexes and safe ranges may locate a problem. Use existing
SDK errors for malformed types/offsets/resources where applicable rather than
catching errors and returning an empty successful result. All invalid inputs,
partial failures and lost acknowledgments preserve the existing failure model.

## Coverage, review order and proof

| Requirement | Target | Contract section / implementation issue |
|---|---|---|
| X2 complete launch draft | `0.5.0-beta.2` | This review and compile-only types/examples; #143 |
| D4 source/review metadata | `0.5.0-beta.2` | D4; [#140](https://github.com/DataFog/datafog-core/issues/140), merged #156; beta.2 unreleased |
| D5 committed recall | `0.5.0-beta.2` | D5; [#141](https://github.com/DataFog/datafog-core/issues/141) |
| D2 additional rules | `0.5.0-beta.2` | [#144](https://github.com/DataFog/datafog-core/issues/144); validate protected inventory, no new rule API here |
| D6/D7 rules/choices | `0.5.0-beta.2` | D6/D7; [#145](https://github.com/DataFog/datafog-core/issues/145) |
| D8 combined review | `0.5.0-beta.2` | D8; [#146](https://github.com/DataFog/datafog-core/issues/146), overlap [#84](https://github.com/DataFog/datafog-core/issues/84) |
| B3 stronger backup | `0.5.0-beta.2` | Snapshot/backup; [#142](https://github.com/DataFog/datafog-core/issues/142), host reauthentication input |
| C1/C3 operations/counts | `0.5.0-beta.3` | C1/C3; [#147](https://github.com/DataFog/datafog-core/issues/147) |
| C2/R3 conversation/restore | `0.5.0-beta.3` | C2/R3; [#148](https://github.com/DataFog/datafog-core/issues/148) |
| D9 CSV | `0.5.0-beta.3` if approved | [#149](https://github.com/DataFog/datafog-core/issues/149); scope decision expected October 29, no implementation authorized yet |
| G1 provider marker preservation | `0.5.0-beta.3` | [#150](https://github.com/DataFog/datafog-core/issues/150); separate measured qualification |
| G2 shipping detection/recall | `0.5.0-beta.3` | [#151](https://github.com/DataFog/datafog-core/issues/151); actual Small/rules/recall corpus, not native scores |
| B2 complete backup coverage | `0.5.0-beta.3` | [#152](https://github.com/DataFog/datafog-core/issues/152); every landed field and supported profile |
| G4 shipping vault review | `0.5.0-beta.3` | [#153](https://github.com/DataFog/datafog-core/issues/153); exact Core, extension and native server hosts; server keys/storage included |
| G3 contract freeze | `0.5.0-beta.3` | [#154](https://github.com/DataFog/datafog-core/issues/154); all four SDK contracts, snapshots, parity, backup, streaming and server tests |
| X6 native Python | `0.5.0-beta.3` | [#167](https://github.com/DataFog/datafog-core/issues/167); complete implemented workspace API, installed wheel |
| X7 native Node | `0.5.0-beta.3` | [#168](https://github.com/DataFog/datafog-core/issues/168); complete implemented workspace API, installed native package |
| X8 workspace parity | `0.5.0-beta.3` | [#171](https://github.com/DataFog/datafog-core/issues/171); fail-closed actual Rust/WASM/Python/Node execution |
| R4 streaming restore | `0.5.0-beta.3` | [#169](https://github.com/DataFog/datafog-core/issues/169); atomicity decision before implementation |
| W6 server-host contract | `0.5.0-beta.3` | [#170](https://github.com/DataFog/datafog-core/issues/170); ownership, durable CAS, worker/retry isolation |

Beta.2 remains the immediate release priority: finish remaining rule slices and release verification before the independent beta.3 work joins the
freeze. Keep examples aligned with main's actual declarations as further APIs land.
CSV waits for Sid's decision. Record approvals in issues/ADRs; merging this draft
does not complete implementation or pilot gates.

### Native, streaming and server work: beta.3 scope

Native Python (#167) and native Node (#168) expose the complete implemented shared
workspace surface in process, with thin bindings, stable errors and original
coordinate ranges. Lifecycle, recall/policy, review, provenance and final streaming
APIs must be tested in installed packages. Browser/declaration tests do not prove
native support. #171 must fail on missing/unsupported/skipped/malformed SDK calls,
comparing deterministic inner snapshots, markers, heads and errors, rather than
host-random ciphertext or unlike models.

Streaming (#169) needs an explicit atomicity decision before implementation:
bounded whole-response buffering preserves all-or-nothing restore; progressive
release must be a separately approved contract because a later damaged marker
cannot retract earlier output. Chunk boundaries, Unicode, EOF, cancellation,
buffers and pinned workspace/head/conversation policy need shared Core tests.
No stream signature is frozen by the type artifact here. Provider SSE/tool parsing
belongs to a separate adapter.

Server-host qualification (#170) requires tenant/request/workspace ownership,
complete-head atomic compare-and-save with exactly one winner for state-changing
candidates, no loser activation/release, uncertain-save reopening and fresh review.
No-op candidates preserve their head; delivery deduplication/admission remains a
host duty, not a consequence of snapshot CAS. The 32-live-plan
cap is per handle; host-wide admission and worker coordination are separate.
Stateless workers require a durable shared mapping store or an authenticated
client-carried encrypted snapshot. Resent chat history alone cannot reconstruct
secret mappings or authorize unknown committed markers. G4 (#153) includes
server key management and storage review alongside extension vault review.

[LiteLLM #22821](https://github.com/BerriAI/litellm/issues/22821) is marked
closed/completed. Its tool-shape, request-state and restoration scenarios motivate
regression tests; these issues do not claim to fix an open LiteLLM bug or implement
an adapter. Native Python [#175](https://github.com/DataFog/datafog-core/pull/175) and Node
[#174](https://github.com/DataFog/datafog-core/pull/174) now have active runtime
candidates, with Node/Python host proof drafts [#178](https://github.com/DataFog/datafog-core/pull/178)
and [#177](https://github.com/DataFog/datafog-core/pull/177). These remain beta.3
review/qualification work, not beta.2 release source or a frozen all-SDK contract.
Streaming and full fail-closed workspace parity still have their own gates.

Compile the [extension-facing examples](launch-api-examples.ts) using the pinned
compiler, without building or emitting future runtime code:

```sh
npm ci --prefix bindings/wasm
bindings/wasm/node_modules/.bin/tsc --project docs/contracts/tsconfig.json
```

The CI type check covers actual main workspace calls and separately named
future beta.3 proposal interfaces/examples. Negative assertions cover unimplemented beta.3 calls only. It proves declaration consistency,
not implemented matching, privacy correctness, cryptography or browser behavior.
Each runtime issue still requires Core positive/negative tests, installed SDK
checks, all coordinate units, packed-SDK MV3 save/restart/recovery tests and the
repository's required Rust/dfv checks. Retain exact tested source/package/model
identities. Feature status follows the linked runtime PRs; this draft does not complete a launch gate.
