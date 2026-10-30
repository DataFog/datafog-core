# Core–FogVault integration contract candidate

This optional Core-owned integration joins the existing Workspace API to the
maintained FogVault native/browser owners. It does not freeze Core's public v1
format or approve either product for production. CSV is deferred from v1 by the
owner's October 30, 2026 decision.

## Component ownership

Core owns mappings, markers, preparation, review, saved heads and activation.
FogVault owns keys, authenticated records and durable publication. The host owns
literal user approval, tenant/request ownership, trusted native control storage,
device-provider qualification and network handoff/observed-send evidence.
The adapter is optional; neither engine acquires a runtime dependency on the
other's application semantics.

## Record binding version 1

One Core workspace occupies one opaque FogVault record. Its payload is the exact
Core bundle exported for that save. Its authenticated metadata is exactly 88
bytes, with no trailing bytes accepted:

| Offset | Length | Value |
| --- | --- | --- |
| 0 | 8 | ASCII `DFVBRG01` |
| 8 | 16 | Core workspace namespace |
| 24 | 16 | FogVault record ID |
| 40 | 8 | Expected parent Core revision, unsigned little-endian u64 |
| 48 | 16 | Expected parent Core commit ID |
| 64 | 8 | Saved candidate Core revision, unsigned little-endian u64 |
| 72 | 16 | Saved candidate Core commit ID |

The metadata version is independent of the Core package/snapshot version. On
brand-new empty workspace creation parent and candidate heads are equal. For a
fresh-destination recovery/import, parent is the authenticated source Core head
and candidate is the actual prepared recovery head, even though the new FogVault
record has no previous record revision. On ordinary changes they
come from the actual prepared change, never from guessed revision arithmetic.
No-op saves follow the same complete-head rules. Namespace and record IDs must
each be nonzero 16-byte values; both codecs reject an all-zero identity. Host IDs use a CSPRNG, never
source-derived values. JavaScript revisions cross the boundary as bigint or
canonical decimal strings; converting through Number is forbidden.

On open/read, require the requested record ID to match the metadata and require
the decoded Core namespace and complete head to match the metadata candidate.
When the caller supplies an expected workspace identity/head, require that too.
FogVault authenticates record/vault identity; this bridge check does not create
anti-rollback authority beyond the admitted native/browser storage profile.
Retain FogVault record revision and full receipt/head separately for its CAS.

## Save and lifetime authority

1. Prepare through the owned Core Workspace without committing it.
2. Show the exact preview locally and obtain explicit approval. Require literal
   true in dynamic-language entry points. Preparation/export alone is no approval.
3. Export the exact candidate and metadata and save with both the current
   FogVault record revision and applicable vault receipt/head fence.
4. Require confirmed persistence of that exact request/candidate before calling
   Core activate_saved / activateSaved with its complete parent/candidate heads.
5. Return handoff permission only after activation and a final owner-generation
   check. Network sends are a separate host action, never an automatic retry.

The owner serializes mutations. It must not expose a path that bypasses its
save/activation sequencing as a supported safe API. The host must not mutate the
same raw handles behind the adapter. Close Core immediately on lock/close and
invalidate its plans/streams before waiting for asynchronous vault cleanup.
Check the owner generation after every await and drop late results. A lock
revokes future authority; caller-owned plaintext copies remain caller-owned.

An uncertain save, conflict, or persisted-but-not-activated result grants no
handoff. Invalidate the Core owner and require authenticated reopen/reconciliation
with a fresh Core preparation and approval. Preserve the exact FogVault request
identity when available. Never infer rollback from a lost response, reuse an old
approval, resend transport automatically, or reinterpret pending send intents.
Known pre-publication rejection may preserve a valid pending plan only where the
actual APIs prove it; conservative owner closure is an allowed safe behavior.

## Migration and evidence

Legacy Core reference-host encrypted backups and FogVault age/FVEXP001 decoded
record exports are different formats with different access/origin policies.
Migration may accept only a separately authenticated/decrypted Core bundle, then
validate its namespace/head and prepare explicit Core recovery for a fresh,
unoccupied destination. Do not invent future v1 bytes or transplant legacy keys.
Production legacy decryption, full destination publication and final v1 migration
approval must be reported separately when not executed by a test.

Native and browser adapters must execute the same binding vectors and failure
rules. Installed candidate tests must use the maintained FogVault APIs, preserve
the exact published Core 0.5.0-beta.3 regression, and identify tested source and
artifact bytes. Synthetic/mocked tests alone are not integration acceptance.
Native integration may use an isolated newer Rust toolchain without changing
Core's existing Rust 1.88 minimum. No changes to FogVault are authorized by this
Core workstream; report a concrete needed external change to the owner first.
