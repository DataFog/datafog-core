# Launch API review draft — 0.5.0-beta.2 / 0.5.0-beta.3

Status: **proposed, not approved or frozen**. This is the X2 contract review for
[#143](https://github.com/DataFog/datafog-core/issues/143). Recommendations below
need maintainer/product review with the extension owner before the corresponding
runtime feature merges. They do not amend [ADR 004](../adr/004-workspace-aliasing-v1.md)
by themselves. Synthetic data remains the integration baseline until the pilot
gates pass.

## Supported runtime versus proposals

| Layer | Status | Consumer entry point |
|---|---|---|
| `0.5.0-beta.1` detection/model and workspace lifecycle | Released runtime | `@datafog/wasm`, `@datafog/wasm/models`; [versioned integration guide](../extension-integration-0.5.0-beta.1.md) |
| D4 sources/review flags, target `0.5.0-beta.2` | Implementation proposed in [draft PR #156](https://github.com/DataFog/datafog-core/pull/156), not released | Review that PR's defaults and selection contract before adopting its pinned candidate |
| D5–D8 recall/policy/combined review, target `0.5.0-beta.2` | Type/behavior proposals in this document | No installed runtime methods yet |
| C1–C3/R3 operations/conversations/restore, target `0.5.0-beta.3` | Type/behavior proposals in this document | No installed runtime methods yet |

The adjacent [proposal declarations](launch-api-proposal.d.ts) are a **type-only
review artifact outside the npm package**. They neither augment the current
`Workspace` declaration nor add runtime exports. A future `LaunchWorkspaceDraft`
value cannot be obtained by casting an installed beta.1 `Workspace`.
Implementation issues must ship actual exports, declarations and versioned
guidance together. Release targets express scope, not a guarantee of current
support. CSV remains a separate unresolved scope decision in
[#149](https://github.com/DataFog/datafog-core/issues/149); this draft has no CSV API.

## Ownership and unchanged lifecycle

Core owns normalization, offsets, marker validation, source propagation, recall,
policy enforcement, selection, transaction heads and snapshot decoding. Rust and
WASM expose the same semantics; JavaScript does transport validation and host
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

## Decisions recommended for beta.2

### D4: declared source and review flags

Adopt PR #156's optional input `source: "scanner" | "recall" | "user"` and
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

Recommend `recall(text): RecallResultDraft`. It reads only committed entries and
returns findings tagged `recall` / `true`, a head and original coordinate ranges.
It allocates no aliases and changes no state. Callers may feed these findings
into existing preparation after D4 ships; the later `prepareReview` recomputes
recall itself. An empty complete result means no eligible committed match, not
that the input is free of PII.

Recommended matching rules for review:

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
  word boundary. For this proposal, word characters are Unicode Alphabetic,
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
  spelling stay separate. Existing preparation still applies its documented
  selection; D8 adds the stronger combined-review policy.

Reuse existing ceilings: 1 MiB source, 20,000 stored entries, 10,000 returned
findings, and 32 inputs/aggregate source/finding limits for batch preparation.
Recommend additional explicit caps of 4 MiB normalized source, 16 MiB normalized
keys and 32 million charged matching transitions/comparisons per recall call.
Charge normalization, matching and candidate emission work; fail the whole call
with `resource_limit` before exceeding a cap. No partial successful result,
skipped entries or silent truncation. **These additional budgets need measurement
and review before being fixed**, using representative maximum-entry, repetitive
and Unicode-expansion inputs on the actual browser adapter. No performance claim
or prescribed matching implementation is made by this draft.

### D6/D7: type policy and remembered choices

Recommend the literal interpretation of ADR 004's **always hidden** types:
`PERSON`, `ADDRESS`, `EMAIL`, `PHONE`, `ACCOUNT_NUMBER`, `CREDIT_CARD`, `SSN`,
`US_ROUTING_NUMBER`, `API_KEY`, `BEARER_TOKEN`, `JWT`, `PRIVATE_KEY`,
`CREDENTIAL_URI`. The exact shipping detector-to-type inventory must be checked
in #144/#145; unknown valid types default to hide.

Protected types accept only the `hide` rule; reject both per-value `keep` and
`not_sensitive` with `protected_type`. For nonprotected types, allow `hide` or
`keep` and exact normalized `(entity type, value)` choices. A value choice overrides
its nonprotected type rule. `forget_choice` removes that exact choice and returns
to its type rule. Cross-type, substring and overlapping wider values remain
unaffected. Keep existing aliases permanent: removing a choice must not delete or
reassign a marker already used in a conversation.

This conservative recommendation means genuine protected-type false positives
stay masked. The alternative is to allow explicit `not_sensitive` assertions on
protected types while still rejecting `keep`; that improves correction UX but
weakens the literal always-hidden guarantee and requires an explicit ADR/product
decision. **Do not implement that exception by treating a source flag, manual
selection, arbitrary host allowlist or detector name as authorization.** This is
the principal product decision in #145.

Recommend read-only `policy()` and transactional
`preparePolicyChange(commands, operation)`. A command batch is atomic and bounded
to 10,000 commands/choices and existing aggregate byte limits. Persist policy and
choices in keys 6/7; validate labels, dispositions, duplicate commands, bounds
and decoding. Reject duplicate commands for the same target rather than choose
last-write-wins. Require a fresh reviewed mutation for remove/update; no hidden
write during recall. Exposing local choice display values is sensitive and
belongs only in the unlocked trusted review surface, never telemetry/errors.

### D8: one Core-owned review list

Recommend `review(input)` as read-only and
`prepareReview(inputs, operation)` as the authoritative transaction. Both validate
scanner inputs and user selections, compute committed recall and apply policy.
The latter recomputes against the current head and source rather than trusting a
previous review list. Caller-supplied `source` does not grant precedence: the
separate scanner/user input channels and explicit correction command establish
the declared intent, all still subject to Core validation and protected coverage.

Recommend deterministic rules:

1. Apply permitted choices to their exact type/value identities. Record remaining
   protected intervals as coverage obligations before resolving overlaps.
2. Exact same type/range duplicates become one candidate; retain a sorted unique
   `contributingSources` list in review results. Choose display source
   `user > recall > scanner`, with stable detector metadata tie-breaks. This
   changes D4's first-input tie only in the new combined-review API. Combine
   `needsReview` with logical OR across retained duplicate contributions; one
   caller's false flag cannot clear another contribution's review request.
3. Prefer the precise native SSN attribution over PERSON on exactly equal spans
   as proposed in [#155](https://github.com/DataFog/datafog-core/pull/155). Do not
   generalize that change into global protected-type ranking or let a source
   label pretend to be native detector evidence. Return `conflicting_types` for
   unresolved equal-span type conflicts rather than invent an ordering.
4. A containing retained span may cover nested candidates only when it replaces
   their full protected text. A manual correction cannot shrink/drop an existing
   protected interval or relabel it into an allowed disclosure. Validate its
   exact source slice and review-local target key; stale keys reject.
5. For partial overlaps whose selection would expose any protected suffix/prefix,
   return ranged `partial_overlap`/`protected_coverage` conflicts; preparation
   rejects with `review_conflict`. Require a fresh covering selection/correction
   and approval. Do not silently choose one finding or introduce a synthetic
   composite entity/marker format in this slice.

Review keys identify the uncorrected candidates before applying corrections; they
are ephemeral, deterministic for the same head/input and detection data, and
contain no raw text. Do not persist or use them as authority. Legacy
`prepare` behavior remains available for compatibility, but #145 must apply the
same protected-policy rules to every preparation entry point. #146 must settle
the protected-coverage behavior of legacy calls too; a new API cannot leave a
host-only bypass. UI highlights use returned UTF-16 ranges; Core owns selection.

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

Recommend 10,000 operation records with no silent eviction or compaction in V1;
reaching the bound is an explicit `resource_limit`, with host guidance to stop
and resolve retention. Approve a retention strategy before raising this bound.
Failed, blocked, cancelled, empty read-only and restored calls do not silently
append operations. A host wanting to record a cancellation/block must explicitly
stage `prepareOutcome`, save and activate it. Losing that optional record never
turns failure into a successful action.

Recommended counts are persisted aggregates of committed records, not activity
telemetry: `operations` increments once per committed operation; `preparedInputs`
counts input members of successful preparation records (including empty findings);
`transformedSpans` counts replaced occurrences, not distinct identities;
`confirmedHandoffs` counts committed host-observed insert/send records, including
repeated handoffs; `reportedBlockedAttempts` counts explicit blocked outcome
records. Reads, previews, retries of a pending plan and discarded candidates add
zero. A no-op **explicitly committed** policy change is an operation but changes
none of the other counts. Stage counters and records atomically in the candidate;
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

Schema 1 remains provisional until [#154](https://github.com/DataFog/datafog-core/issues/154).
D4 metadata is transient; recall reads current entries. Policy/choices add keys
6/7; operations/conversations/counts add 8/9/10; key 5 remains reserved for future
merges. Each implementation must add strict decoder and encrypted round-trip
fixtures for its fields. No silent defaulting of missing new required fields or
discarding unknown fields to open incompatible development bundles. Define
supported older versions explicitly; otherwise reject and require a documented
development reset. The freeze must choose the final schema/version/migration
policy and public contract constant; this draft exports no fake version constant.

The host backup proposal requires **fresh password entry and verification for
every export**, even from an unlocked vault. `freshPassword` is an input obligation,
not a type-level proof of freshness; the trusted host must reauthenticate before
export. The recorded [#142 decision](https://github.com/DataFog/datafog-core/issues/142)
is Argon2id 256 MiB / t=3 / p=1, v2 strict export
profile, no weaker inner password wrapper, with explicit legacy imports. Do not
silently downgrade a profile to handle low-memory devices; fail explicitly.
The adjacent `BackupHostDraft` is a host contract proposal, not a WASM export.
An unlocked recovery-only user without the password receives a clear
credential-required failure; no forgotten-password reset/export bypass is implied.
Imported backups need the approved profile/version policy and fresh destination
recovery head/key flow. Never log credentials or keep them in workspace snapshots.

## Proposed calls and failure behavior

The type artifact is the exact proposed call shape; the table summarizes its
behavior. All new error names below are **proposals**, not current SDK exports.

| Proposed call | Ownership and result | Failure/compatibility decision |
|---|---|---|
| `recall(text)` | Core read; head + complete original-coordinate candidates | Existing `invalid_marker`/`invalid_finding`/`resource_limit`; no partial results |
| `review(input)` | Core read; candidates, attribution and ranged conflicts | Invalid input rejects; resolvable overlaps appear as conflicts |
| `policy()`, `operations()`, `counts()`, `conversations()` | Trusted local reads of committed state only | Closed workspace rejects; lists obey saved-state bounds |
| `prepareReview(inputs, operation)` | Core mutation plan; validates and recomputes the approved source | `review_conflict`, existing range/head/resource errors; no trusting a stale review |
| `preparePolicyChange(commands, operation)` | Core metadata plan; zero previews | Proposed `protected_type`; bad/duplicate commands reject atomically |
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
| D4 source/review metadata | `0.5.0-beta.2` | D4; [#140](https://github.com/DataFog/datafog-core/issues/140), draft #156 |
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
| G4 shipping vault review | `0.5.0-beta.3` | [#153](https://github.com/DataFog/datafog-core/issues/153); exact Core and product extension builds |
| G3 contract freeze | `0.5.0-beta.3` | [#154](https://github.com/DataFog/datafog-core/issues/154); after fields, backup and provider tests |

Review D4 and this draft first; then resolve D5 matching/budgets and D6/D7 protected
choice semantics before implementing their dependent D8 work. Operations drafting
can proceed independently; conversation semantics require that transaction model.
CSV waits for Sid's decision. Approval of a section must be recorded in the
corresponding issue/ADR; a merged draft document alone does not settle every
product question or complete a feature.

Compile the [extension-facing examples](launch-api-examples.ts) using the pinned
compiler, without building or emitting future runtime code:

```sh
npm ci --prefix bindings/wasm
bindings/wasm/node_modules/.bin/tsc --project docs/contracts/tsconfig.json
```

The CI type check includes current beta.1 calls and negative assertions that
current `Workspace` lacks future methods. It proves declaration consistency,
not implemented matching, privacy correctness, cryptography or browser behavior.
Each runtime issue still requires Core positive/negative tests, installed SDK
checks, all coordinate units, packed-SDK MV3 save/restart/recovery tests and the
repository's required Rust/dfv checks. Retain exact tested source/package/model
identities. No feature or launch gate is marked complete by this draft.
