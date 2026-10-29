# Workspace backup field coverage

This is the current-field inventory for [#152](https://github.com/DataFog/datafog-core/issues/152).
It covers supported Core snapshot versions 1 and 2. It does **not** complete the
full 0.5.0-beta.3 backup gate: operation records, event counts and conversation
state still require their own implemented schema, fixtures and encrypted recovery
qualification. A proposal or reserved key is not persisted runtime support.

The inner canonical CBOR snapshot version, encrypted vault version and exported
backup-file version are independent contracts. A current backup file uses version
2 and password profile 2 (Argon2id 256 MiB, three passes, one lane). This does not
imply that its inner snapshot contains every planned beta.3 field.

## Supported inner fields

The authoritative codec is [workspace/codec.rs](../../crates/core/src/workspace/codec.rs).
V1 has exactly keys 0–6; V2 has exactly keys 0–7, in canonical order. Unknown,
duplicate, missing or noncanonical fields reject rather than being discarded.

| Key / member | Supported content and recovery behavior | Concrete proof |
| --- | --- | --- |
| 0 | Snapshot version: 1 or 2. Legacy reads/no-op preparations preserve V1 bytes; an approved saved mutation or recovery upgrades to V2. | `legacy_snapshot_and_reserved_empty_slot_stay_byte_exact_until_saved_upgrade`; both-credential legacy encrypted upgrade in `verifyPolicyHost` |
| 1 | Namespace: 16 bytes, preserved across backup/recovery so old full markers retain their meaning. | Current fixture byte preservation and encrypted `verifyCurrentFieldBackups` |
| 2 | Head: exact u64 revision plus 16-byte commit ID. Import approval creates a fresh recovery head, rather than copying the old concurrency identity. | Core recovery test; encrypted fresh-head assertion; legacy high-counter fixture preserves values above JavaScript's exact integer range |
| 3 / entry[0] | Exact entity type label. It participates in identity and marker matching. | Current fixture contains PERSON and ACCOUNT_NUMBER; per-alias restore and missing-entry-member rejection |
| 3 / entry[1] | First saved display spelling. Normalized identities are reconstructed from it; Unicode case/space variants do not replace it. | `İ Jane` fixture, original-alias restoration and recall after encrypted recovery/restart |
| 3 / entry[2] | Canonical six-character Crockford code. Existing aliases are permanent. | Exact current fixture bytes and marker restoration; existing duplicate/noncanonical-code decoder tests |
| 3 / entry[3] | 16-byte issuance ID. Current retained IDs continue to prevent reuse after recovery. | Current live-ID rejection after a different recovery head; missing issuance rejection |
| 3 / entry[4] | Entry's nonzero u64 allocation counter. | Fixture has two PERSON identities and one ACCOUNT_NUMBER; Core next-person allocation continues at 3 |
| 4 / counter[0–1] | Type label and its u64 allocation high-water mark, consistent with entries. These are **alias allocation counters**, not event/operation statistics. | Exact recovered field bytes, next-allocation test, `counters.json` high/maximum fixtures and missing tuple-member rejection |
| 5 | Reserved merge slot: **only the empty array is supported**. No workspace merge state is implemented. | Six wrong/nonempty/indefinite/oversized values, duplicate key, unknown replacement key and missing key reject; authenticated nonempty/wrong-type import also rejects |
| 6 / policy[0] | Profile: `synthetic_v1` in V1 or `protected_v1` in V2. | Existing policy-profile rejection tests; current fixture and missing-profile rejection |
| 6 / policy[1] | V1: empty array. V2: rule records `[type, rule code, operation ID]`. Protected types cannot receive a generic keep rule. | Nonempty hide/keep rules, each missing rule member, missing rules field and existing forged-permission tests; encrypted byte/settings preservation |
| 7 / choice[0–3] | V2 only: `[type, first display spelling, disposition code, operation ID]`. Exact normalized choices include explicit protected-type `not_sensitive` and allowed custom-type `keep`. | Both dispositions in the nonempty fixture; every missing choice member; exact settings/alias preservation with both backup credentials |

The synthetic [current-field fixture](../../fixtures/workspace/backup-current-fields.json)
contains three committed identities, two type counters, two rules and two exact
choices. Its 34 negative vectors remove each supported container/tuple member or
violate the reserved-slot contract while leaving the other representative state
intact. Core tests verify the declared field byte boundaries against the actual
CBOR decoder. The packed WASM test consumes those same vectors through public
`Workspace.openBundle`, not a JavaScript replacement decoder.

Current operation IDs in entry/rule/choice records are not a historical operation
ledger or a general replay guarantee. Source/review flags, query offsets, pending
plans, approval state, live streaming carry and partial-delivery counts are
transient; they are not serialized as hidden backup fields. Policy views also
include derived runtime defaults/protected-type lists, which are not separately
persisted arrays.

## Authenticated recovery coverage

`verifyCurrentFieldBackups` runs the packed WASM SDK in the actual MV3 reference
extension, with IndexedDB, browser AES-GCM and the pinned KDF worker. It exports
nonempty V2 state, imports with **both password and recovery code**, approves the
save, closes/restarts the browser and unlocks again. Every supported field except
the deliberately new head is byte-preserved. Aliases, original Unicode spelling,
recall, rules and choices remain intact. An independent native Node HKDF/AES-GCM
check confirms a genuinely different destination workspace key.

Five representative malformed inner snapshots are first encrypted and exported
with a valid backup authentication tag, then rejected by the actual import UI:
missing entries, missing choices, missing policy rules, nonempty merge state and
a wrong-type merge slot. Each rejection leaves IndexedDB unchanged, clears the
preview and grants no approval. Thus these cases test Core state validation after
authentication, rather than merely failing a damaged ciphertext tag.

The existing backup/policy suites additionally cover occupied destinations,
rollback/lost acknowledgement, cancellation, competing imports, malformed outer
metadata, credential-slot tampering, version/profile downgrade rejection,
password export reauthentication, and legacy backup-file imports. Legacy **inner
V1 snapshots** are also recovered with both credentials, with an upgrade warning
before approval and preserved old aliases. Core V1 no-op reads remain byte-exact.

These are functional/state/authentication proofs in the reference host. They do
not certify the shipping extension or complete [#153](https://github.com/DataFog/datafog-core/issues/153).
Host credentials, key persistence and memory handling retain their separate review.

## Required additions before #152 can close

| Planned state | Current status | Coverage still required |
| --- | --- | --- |
| Operation records and event counts (#147) | Not implemented in the supported V1/V2 codec; approved product details do not add runtime fields. | Final accepted schema/limits, nonempty records/counts, recovery/replay semantics and encrypted round trips |
| Conversations/provenance (#148) | Not implemented in the supported V1/V2 codec. | Relationships, aliases across conversation scope, restore behavior and encrypted recovery fixtures |
| Reserved merge key 5 | Empty-only rejection is covered; post-V1 merge state is outside this scope. | No merge feature should be inferred from reservation |
| CSV-specific state (#149) | Awaiting the separate CSV decision; no new field is assumed. | Add fixtures only if an approved implementation persists new state |

Every added persisted field must extend this inventory and its Core/packed-SDK/
encrypted backup tests. The final B2 gate stays open until all intended launch
state exists and that complete matrix passes.

Run after doctor/build:

```sh
cargo test -p datafog-core --test workspace_backup_fields
node scripts/test-wasm-package.mjs
node scripts/test-workspace-extension.mjs
```
