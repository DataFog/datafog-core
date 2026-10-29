# D6/D7 protected policy and remembered choices

Concrete draft for `0.5.0-beta.2`, tracked by #145 and stacked on #164/#156.
Sid approved the narrow protected-type exception and saved-format upgrade behavior
on 2026-10-28 (America/Los_Angeles). This branch does not merge or publish a release.

## Approved behavior

All types default to hide. Protected types reject generic `keep` at both the
per-type and per-value levels. The protected catalog covers every built-in PII
type and the native/model names and addresses (`PERSON`, `FIRST_NAME`,
`LAST_NAME`, `ADDRESS`, `STREET_ADDRESS`, `ACCOUNT_NUMBER`, `CUSTOMER_ID`). Valid
custom types may use an explicit keep rule or keep choice; they still default to
hide. The versioned `protected_v1` catalog is available through `policy()`.

An explicit `not_sensitive` false-positive correction may leave the exact
normalized `(entity type, value)` visible, including protected types. Core cannot
prove the correction truthful: actual PII can deliberately be overridden. A
source or needs-review flag never grants this exception. This does not introduce
field/reference authorization or arbitrary caller-selected disclosure rules.

A choice may precede alias issuance, so correcting a newly scanned false positive
does not allocate a code. Identity uses existing Unicode lowercasing and collapsed
Unicode whitespace; no punctuation, transliteration, fuzzy or cross-type match is
added. Validate every supplied finding before filtering waived identities, then
apply existing overlap selection. A narrower or differently typed correction
cannot waive a wider protected span. All single, batch and recall paths consult
the same Core policy. Recall remains read-only and an empty result is not a
clean-text verdict.

Choices can be updated or removed. Type rules can be reset. These edits affect
future preparation and recall; previously issued codes, counters, first displays
and restore mappings remain permanent. Reallowing protection reuses an old code.
Strict source marker preflight and all-or-nothing restoration still apply.

## Transactions and API

Rust exposes `PolicyEdits`, `TypeRuleEdit`, `ValueChoiceEdit`, `TypeRule`,
`ChoiceDisposition`, `PolicyView`, `PolicyPreview` and `SnapshotFormat` through
`workspace`. The matching WASM call is:

```ts
const change = workspace.preparePolicyChanges({
  rules: [],
  choices: [{entityType: "PERSON", value: "May", disposition: "not_sensitive"}],
}, randomId());
const review = change.policyPreview(); // Sensitive local before/after values.
const format = change.snapshotFormat();
// If currentVersion=1 and candidateVersion=2, explain beta.1 incompatibility now.
const heads = change.heads();
const encrypted = await encryptForHost(change.exportCandidateBundle());
await saveAtomically(encrypted, heads.expectedHead, heads.candidateHead);
workspace.activateSaved(change, heads.expectedHead, heads.candidateHead);
```

`rule` is `hide`, `keep` or `null` (reset). `disposition` is `keep`,
`not_sensitive` or `null` (remove). `policy()` returns current settings and saved
snapshot version. Policy plans have no text previews or handoff permission;
`policyPreview()` contains local before/after edits. Exact no-op edits preserve
head, bytes and version. Candidate exports are immutable and retryable after a
definite failed save. Activation requires the complete expected/saved heads,
belongs to the same runtime and consumes the plan once. Discard, lock, failed
save, stale parent and getter calls do not change committed policy. A lost save
acknowledgment requires reopening saved state before retrying.

The host owns approval, encryption and durable storage. Core cannot verify a
host's claim of atomic persistence or truth of a false-positive correction. All
policy enforcement is in Core, including legacy APIs; UI metadata is not the
enforcement boundary. Errors are content-free: malformed/forbidden edits return
`invalid_policy`; resource, lifecycle and head errors use existing workspace codes.

## Authoritative candidate policy-v2 snapshot

The prior implementation had keys 0–6 only, with a fixed key-6 placeholder
`{0: "synthetic_v1", 1: []}`. ADR 004's future key allocation was not implemented
policy persistence. This branch introduces the authoritative candidate beta.2
version-2 meaning; the separate unmerged #162 operation proposal must rebase and
version coherently, not ship another incompatible version 2. No beta.3 operation
code or keys are imported here.

The inner snapshot is a canonical, definite eight-key CBOR map:

| Key | Policy-v2 meaning |
| --- | --- |
| 0 | Version `2` |
| 1–5 | Existing namespace, head, permanent entries, counters and empty merge reservation |
| 6 | `{0: "protected_v1", 1: [[type, ruleCode, operationId16], ...]}` |
| 7 | `[[type, display, dispositionCode, operationId16], ...]` |

Type rules use `0=hide`, `1=keep`. Choices use `0=keep`, `1=not_sensitive`.
Normalized keys are derived from stored display values, never separately trusted.
Rows are sorted by their existing identity keys. The decoder rejects protected
keep records, unknown profiles/codes/keys, duplicates (including normalized
identities), indefinite/noncanonical/truncated data, invalid labels/markers,
inconsistent heads and oversized data before admitting state. It does not accept
or drop unimplemented operation/conversation fields.

Limits are 10,000 edits per call, 20,000 explicit rules and 20,000 choices, a
1 MiB value ceiling and the existing 16 MiB aggregate snapshot/transport ceiling.
Reaching a limit fails atomically; there is no silent eviction. These are limits
on current settings, not an operation-history retention policy. The runtime
rejects current head, permanent mapping issuance and surviving settings operation
IDs. It does not claim a complete historical operation ledger: IDs no longer
represented after an update/removal cannot all be reconstructed from old state.
Future operation records and their capacity/compression remain separate work.

## Approved compatibility and upgrade notice

Existing version-1 vaults open and re-export byte for byte. Reads, no-op
preparation, exact no-op settings edits and discarded candidates do not upgrade.
A real saved mutation or recovery stages version 2 while preserving aliases and
counters. New workspaces bootstrap version 2. The outer encrypted vault and
backup formats are unchanged and carry the complete inner snapshot.

**Before approving a v1→v2 save, tell the person that beta.1 cannot open the
upgraded vault.** Existing backup files remain unchanged. The reference host
shows this notice in text/settings previews and backup recovery previews only
when the pending change crosses that boundary. This compatibility behavior was
approved by Sid; it is not a remaining blocked design question. Root still owns
code/CI review and release gating.

## Evidence

Rust coverage includes protected/default policy, newly observed corrections,
Unicode identity, type/wider-span scope, every preparation path, recall,
updates/removal, permanent aliases, stale/failed saves, locks, strict decoder
fixtures, settings capacity and legacy byte preservation. Packed-WASM tests
exercise matching declarations, mutable getter copies, malformed inputs and
accessor/array/resource bounds.

The actual packed-SDK MV3 harness uses real Web Crypto and IndexedDB: a failed
settings save leaves policy/head unchanged; retry activates once; lock/reopen
preserves settings; both password and recovery-code backup restore/restart carry
rules, choices and permanent old aliases. A seeded encrypted v1 vault proves the
pre-upgrade notice, read/no-op/discard/failed-save preservation and approved saved
upgrade. These are synthetic-only integration proofs, not actual-person pilots.
