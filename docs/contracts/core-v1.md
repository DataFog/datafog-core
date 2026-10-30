# Core v1 compatibility contract

Engineering compatibility decision: October 30, 2026. Workspace contract **1**
freezes the reviewed existing behavior and saved bytes, with the additive compiled
contract getter described below. It does not redesign markers or snapshots.
Fresh installed Rust/Python/Node/WASM candidates passed 45 workspace cases and
600 steps; the native registry consumer also passed its ten named cases,
including exact published beta.3 interoperability.

This is the compatibility policy for the 1.0.0 candidate, not release approval.
Publication still requires every exact-head platform, unified-package, model and
joint release check. A supported-path failure blocks release and must be fixed
without silently changing this contract. External provider and shipping-extension
acceptance remain separate under the owner-approved Core release scope.

The executable inventory is [core-v1.json](core-v1.json). Run
`python3 scripts/check-core-contract.py`; the installed four-SDK workspace parity
profile also requires the Rust-backed contract identifier before exercising the
existing cases. The [beta.3 inventory](launch-api-0.5.0-beta.2-beta.3.md) remains
the detailed method, field, limit and error-semantics reference. Its historical
release status and provisional language are not evidence of a completed v1 freeze.

## Distinct versions

| Identifier | Candidate value | Meaning |
| --- | --- | --- |
| Package release | 1.0.0 | Semantic version of the shipped distribution |
| Workspace contract | 1 | Workspace behavior/API compatibility family |
| Workspace snapshot | write 3; read 1, 2, 3 | Canonical CBOR saved workspace schema |
| FogVault bridge | `DFVBRG01` / 1 | Separate authenticated workspace storage envelope |
| Parity fixture schema | 1 | Test command/result protocol, not a storage version |

Rust exposes `workspace::WORKSPACE_CONTRACT_VERSION`; Python exposes
`workspace_contract_version()`; Node and WASM expose `workspaceContractVersion()`.
The functions return the value from the compiled Rust Core, not a JavaScript
copy. WASM callers must first `await init()`; calling the getter before init
throws the same initialization error style as other synchronous WASM functions.
The model capabilities `contract_version` is a separate protocol identifier.
This additive getter is absent from beta.3; existing beta.3 calls and bytes stay
compatible. Consumers requiring the getter must depend on the new package.

## Compatibility policy

Within v1, existing public runtime names, documented argument/result shapes,
error codes, fail-closed behavior, exact head comparisons and saved bytes remain
compatible. Additive optional APIs may arrive in minor releases. A change that
breaks these guarantees requires a major API contract decision and an explicit
migration path. Resource exhaustion stays a controlled failure; no successful
result is promised for arbitrary-size inputs or arbitrary hosts.

New workspaces write canonical v3 (map11, integer keys 0–10). Imported v1/v2
workspaces preserve their exact bytes during reads, discarded plans and genuine
no-ops. An approved saved mutation or explicit recovery upgrades them to v3;
upgrade is visible in the plan's snapshot-format metadata and still requires the
normal compare-and-save/activation sequence. Future or unsupported schema
versions reject with `invalid_snapshot`; they are never guessed or silently
rewritten. Encoding is canonical, bounded and strict, including reserved key 5's
empty array. The manifest indexes an existing missing-field rejection for every
current top-level key. The decoder suites additionally cover nested fields,
ledger/conversation invariants, duplicate/unknown fields, truncation, trailing
bytes, non-minimal integers, indefinite maps and legacy upgrade preservation.
These tests already run against actual installed packages; they are not replaced
by source matching.

The host owns authentication, tenant/site authorization, credentials, encryption,
durable atomic storage, real approval and evidence of sending. Core owns marker
identity, ranges, recall, policy, protected coverage and the transaction state
machine. Saving compares the complete revision plus commit ID; activation follows
confirmed durable save. Failure or uncertain acknowledgment releases no approved
text and requires reopening durable state. JavaScript revisions remain lossless
canonical decimal strings; Python/Rust retain exact integers. Strict restoration
releases no prefix on errors. Progressive restoration intentionally reports any
already delivered bytes and complete coordinate metadata; it is not strict mode.

The raw Rust API uses caller-owned plan values and has no global 32-plan limit.
Managed Python/Node/WASM workspace registries cap outstanding plans at 32 per
workspace. The FogVault integrated owners admit one pending mutation; their
stricter lifecycle is a separate adapter constraint. Source/text/bundle and
history limits remain the values enforced by the documented Core implementation;
this contract does not turn adapter limits into universal Rust promises.

## Public surface and evidence

The manifest records each root JavaScript runtime export and Python `__all__`
entry, their declaration coverage, all 19 workspace error codes and required
methods/cases from the existing foundation, operation, conversation and streaming
fixtures. It also retains SHA-256 digests of the reviewed public Node/WASM
TypeScript declarations and Python stub, normalizing line endings to LF. A method
signature, result field, enum or type change therefore requires an explicit
inventory review; matching root export names alone cannot qualify it. These
hashes are an approval checkpoint for the public contract, not proof that runtime
behavior matches a declaration. Installed TypeScript consumers and runtime parity
remain required. Node's declaration generator removes private transport helper value declarations
that have no root runtime export. Negative installed TypeScript imports enforce
that boundary; public runtime exports retain their declarations.
Existing model, detection, structured and transformation contracts remain linked
from the inventory; this workspace decision does not remove them or invent a
second model/provider contract.

The four-SDK fixture profile is documented in
[workspace parity](../workspace-parity-0.5.0-beta.3.md). Retained pre-v1 artifact
evidence establishes a baseline, not a result for fresh v1 packages. Final
qualification must run against the final candidate's Rust, installed Python
wheel, installed native Node package and browser WASM tarball with receipts tied
to the candidate, alongside the normal verifier and integrated FogVault checks.

Core's marker grammar and deterministic failure handling can be frozen without
claiming a provider preserves markers. **No real ChatGPT/Claude/Gemini provider
preservation run is established here, and no ≥99% provider accuracy claim is
made.** Provider behavior and the shipping extension's real-host acceptance remain
unfinished product evidence. Their pending/waiver disposition must be explicit in
release tracking; a source-level Core pass does not close those product claims.

## Optional Core-owned FogVault integration APIs

The same freeze review includes the independently installed Core-owned adapters,
not only the base SDK. Core-owned integration packages use version 1.0.0 and
retain the reviewed v1 interface; FogVault's own version is independent. The
manifest's `integrationBindings` checks exact root value exports and LF-normalized
public declaration hashes for:

| Package | Public runtime values | Reviewed declarations / installed consumer proof |
| --- | --- | --- |
| `@datafog/core-fogvault-node` | `DataFogVaultError`, `NativeWorkspace`, `PreparedChange`, `RestoreStream` | [types](../../integrations/fogvault-node/index.d.ts), [consumer](../../integrations/fogvault-node/test/consumer.mjs), [TypeScript](../../integrations/fogvault-node/test/types.ts) |
| `@datafog/core-fogvault-browser` | `FogVaultWorkspace`, `WorkspaceVaultError` | [types](../../integrations/fogvault-browser/index.d.ts), [browser consumer](../../integrations/fogvault-browser/test/browser.mjs), [TypeScript](../../integrations/fogvault-browser/test/types.ts) |
| `@datafog/core-fogvault-legacy-browser` | `LegacyBackupImport`, `LegacyImportError` | [types](../../integrations/fogvault-legacy-browser/index.d.ts), [browser consumer](../../integrations/fogvault-legacy-browser/test/browser.mjs), [TypeScript](../../integrations/fogvault-legacy-browser/test/types.ts) |
| `datafog-fogvault` | `FogVaultWorkspaceError`, `WorkspaceHead`, `NativeWorkspace`, `PreparedChange`, `RestoreStream`, `__version__` | [stub](../../integrations/fogvault-python/datafog_fogvault.pyi), [installed tests](../../integrations/fogvault-python/tests/test_installed.py) |

The Python inventory checks explicit PyO3 registrations and public names; normal
Python module metadata is not part of that value list. Its runtime version must
match installed distribution metadata. Browser private helper files and native
transport handles are not root public APIs.

Rust's [native owner source](../../integrations/fogvault-native/src/lib.rs) is the
canonical source API for `Binding`, `Error`, `Result<T>`, `Change`, `Committed`,
`Owner`, `BINDING_BYTES`, and the explicit `core` / `vault` re-exports. Review must
cover all public signatures, borrowed/generic types, DTO fields and error
variants, rather than treating private implementation source hashes as an ABI.
The public DTOs are:

- `Binding`: namespace, record ID, complete parent and saved heads; exact 88-byte
  `DFVBRG01` encoding and strict decoding.
- `Change`: opaque, owner-bound prepared candidate; previews, heads, candidate
  size, snapshot format, policy preview, send intents and discard.
- `Committed`: saved Core head, FogVault receipt and caller-owned plaintext
  preview strings; it proves local persistence/activation, not network delivery.
- `Owner`: create/open/import and identity reads; preparation/review/policy/
  recovery/operation/conversation methods; commit/reconcile; counts/history/
  paging; strict and progressive restoration; lock and child invalidation.

The native suite includes ten external consumer tests plus one internal lost-ack
fault test (11 total). [Consumer tests](../../integrations/fogvault-native/tests/native.rs)
exercise exact bridge vectors, beta.3 interoperability, persistence/reopen,
foreign-owner and saved-head rejection, lifecycle invalidation, recovery,
conversation authority, facade coverage and payload admission. The internal test
in the owner source exercises actual publication followed by lost acknowledgment.
Both source API tests and installed Node/Python consumers remain required; this
inventory is not a claim of stable Rust binary ABI or exhaustive automated Rust
signature analysis.

Rust's [migration source](../../integrations/fogvault-migration/src/lib.rs) exposes
`MigrationError`, opaque `RecoveryCandidate`, and `stage_authenticated_bundle`.
The candidate exposes namespace, source/candidate heads, snapshot format and
borrowed candidate bytes. It grants no persistence or handoff authority. The
caller must authenticate/decrypt the source before staging, and the destination
must atomically require an empty record, re-encrypt and reopen durable state.
[Recovery tests](../../integrations/fogvault-migration/tests/recovery.rs) retain
v1/v2/v3 semantics, full binding checks, malformed-source rejection and exhausted
or reused authority behavior. Changes to either Rust integration's public
signatures, DTOs or method coverage require explicit source-contract review even
when the machine-readable linked-file check still passes.

## Archive distribution evidence

The joint Chromium job also runs `python scripts/qualify-native-registry.py`
after preparing the pinned clean FogVault checkout. This shared script builds
actual `.crate` archives, validates approved registry dependency identities and
runs the native consumer against a fresh local candidate registry. CI retains
its report, source/dependency graph proof, command logs, archives and consumer
lockfile, without retaining the entire fresh Cargo home.

The native language matrix retains target-tagged Node binaries, checksum/source
manifests and their successful installed-consumer reports. A required unified
archive job rejects missing/duplicate targets, source or version mismatches and
bad checksums, then installs and tests the assembled tarball without rebuilding
its native code. This preserves the existing three runner scopes, rather than
claiming every base SDK target. The manifest records the observed macOS
architecture; Linux support explicitly requires x64 GNU libc. Python wheel
qualification checks the exact MIT license notice and reviewed README in wheel
metadata. These jobs prepare and qualify candidates; they never publish them.
