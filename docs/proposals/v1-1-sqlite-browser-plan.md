# v1.1 browser Workspace storage with SQLite3 Multiple Ciphers

Status: proposed implementation plan, 2026-11-04. No implementation or qualification is claimed.

## Requirements restatement

Ship SQLite3 Multiple Ciphers (SQLite3MC) as an optional second encrypted storage integration alongside FogVault. Start with the actual browser extension using `@datafog/wasm` Workspace. Core continues to own reversible mappings, review, markers, conversation state, and saved activation. SQLite3MC persists the complete Core bundle; it does not implement a second pseudonymization algorithm.

The extension's bundled CNN is a separate mandatory v1.1 workstream. Its real findings must traverse this backend's review/save/reopen/restore path, with the CNN enabled by default and explicitly disableable by Josh's UI. Disabling detection must preserve restoration of previously saved mappings.

Preserve Workspace contract 1 and published beta.3 interchange. Offer an optional browser adapter, provisionally `@datafog/core-sqlite-browser`, rather than making SQLite3MC a dependency of every Core import. Browser support initially means the shipping Chromium Manifest V3 extension; other engines require named independent qualification. Native SQLite3MC integrations are follow-up scope.

## Existing contract to retain

The canonical inputs are `docs/integrations/fogvault-contract.md`, the actual FogVault browser owner, `docs/core-v1-release-scope.md`, and `docs/browser-model.md`. Reuse the public lifecycle semantics and shared head/identity vectors; do not couple the new backend to FogVault's proprietary receipt format or reuse its `DFVBRG01` magic for a different storage protocol.

The adapter exclusively owns Core and the database connection, admits one preparation, and serializes changes. The host supplies authenticated routing, credentials, exact local approval, and trusted send observations. A successful save permits handoff; sending remains a separate host operation.

## Implementation phases

### 1. Prove the selected browser storage profile

Pin a reviewed SQLite3MC source release, binary/source hashes, build toolchain, license notices, cipher parameters, and baseline WASM assets. Current upstream browser bundles are evidence that a port exists, not proof of our persistent storage profile. Run a small disposable MV3 extension experiment with a dedicated Worker and the chosen persistent OPFS VFS before building the adapter. Prove database creation, encrypted writes, close, browser-process restart, and reopened reads using actual packaged assets. Verify temp/journal/WAL behavior and actual VFS flush/commit semantics for the selected build.

Prefer a profile that works without SharedArrayBuffer or cross-origin isolation because the extension must not depend on page headers. If the selected OPFS VFS requires those features, explicitly establish their availability in the shipping extension or select another maintained VFS. Do not assume synchronous OPFS access, SQL transaction support, or encryption of all persistent files from ordinary SQLite behavior alone. Record browser versions and extension/offscreen-page Worker lifetime assumptions.

Deliver an ADR selecting one cipher with authenticated integrity, one VFS, one journal mode and durability policy. Disable weak legacy cipher choices in the supported API; never silently open a plaintext database or fall back to memory. Set finite database/bundle/KDF/input limits before allocation. Phase passes only after the persistent encrypted database survives real restart in the selected extension profile.

### 2. Define vault metadata, credentials, and storage binding

Use a versioned schema with a vault descriptor and one complete Workspace row per opaque random workspace record ID. Store exact Core bundle bytes, namespace, full parent/candidate heads, adapter storage revision, and save request ID together in the same transaction. Define the encoding and validate it strictly, keeping u64 values as bigint/canonical decimal across JS boundaries. Namespace, vault ID, and record IDs must be random, nonzero identities and must be checked against trusted open expectations.

Keep the identity/head binding inside authenticated encrypted storage. Document the cipher's actual protection granularity: page integrity alone does not establish row identity, semantic consistency, or rollback protection. Decode the bundle and require its namespace and complete head to equal the stored envelope. Reject transplanted/wrong-record bundles, malformed envelopes, and unknown incompatible versions. Store public routing identifiers outside the database only when necessary; never expose mapping values or unwrapped keys there.

Use a randomly generated database key. Define a reviewed password-based wrapping/KDF envelope, authenticated parameters, resource admission, and an independent recovery credential path. Unlock unwraps the database key; password change rewraps it without rewriting Core mappings. Confirm how a bootstrap wrapping envelope is atomically updated and recovered after interruption. This component must use established cryptographic primitives with pinned implementations; SQLite3MC does not itself supply a complete vault credential/recovery design. If an existing maintained wrapping component can be reused without depending on FogVault's owner, document that reuse. Otherwise include the new wrapper format and security review in scope.

Define recovery and encrypted export before shipping: fresh destination, independent key material, authenticated source identity/head checks, explicit approval, and exact restore proof. Lock/close immediately revokes Core authority and all outstanding plans/streams, increments generation, then terminates/releases the Worker and database keys. Avoid promises of erasing plaintext strings already returned to callers. Worker ownership and key-buffer clearing reduce lifetime but do not guarantee browser memory erasure.

### 3. Implement the Workspace owner and conditional save

Expose create/open, prepare/review/discard, approved commit, scoped restoration, lock/close, and fresh-destination recovery through one owner. Follow the existing typed plan surface without exposing raw Core or raw mutable database handles. Preserve Core's conversation/send-intent checks.

Save sequence:

1. Prepare the exact candidate and display its local preview; require literal `approved: true` for that still-current plan.
2. Copy caller-owned expectations and request identity before asynchronous work. Capture owner generation and exact parent/candidate heads.
3. Begin the admitted database transaction. Require expected adapter storage revision and complete saved Core head, then atomically store the candidate bundle, binding, and request identity. Creation must require an unoccupied record; imports use an explicitly fresh destination.
4. Await the selected VFS's confirmed commit and persistence boundary. Inspect update count/transaction result; an SQL call returning alone must not be assumed to prove the intended durability contract.
5. Recheck generation, then call Core `activateSaved` with exact heads. Recheck generation before returning handoff permission.

Use backend locking plus conditional update predicates, not an in-memory JS mutex alone, for multiple owner/tab conflicts. A lock error may be returned as busy; do not automatically rebase or reapply an approved mutation onto newer state. CAS operates on storage revision separately from Core revision, including no-op saves.

States are locked/closed, opening, ready, prepared, saving, and reconciliation-required. Lock is allowed to revoke any active state. Conflict, submitted save with uncertain outcome, lost commit reply, cancellation after publication may have begun, or persisted-but-not-activated state closes Core and grants no handoff. Reopen authenticates durable state and reconciles the exact request ID/candidate identity. Require fresh preparation and approval even when the previous save is found. Never infer rollback from a missing reply or automatically retry a network send.

### 4. Migrate and integrate the extension

Keep FogVault selectable. Import via an authenticated/decrypted Core bundle under the existing recovery contract into a new SQLite3MC destination with a fresh Core recovery head. Do not copy source key wrappers, transplant FogVault receipts, overwrite occupied state, or promise raw ciphertext interoperability. Preserve aliases, policy, mappings and pending send semantics. Reverse migration can follow the same approved recovery boundary once qualified.

Bundle SQLite3MC JS/WASM, Worker, Core WASM and CNN entirely inside the extension. Test emitted tarball assets and bundler-relative paths under the production CSP, with no CDN, remote executable code, or model downloads. Use an extension-owned page/offscreen host and dedicated database Worker; an MV3 service worker is not the durable owner. The CNN runs in its dedicated detector Worker with a scanner-only Core instance; the SQLite adapter exclusively owns the mutable Workspace instance. An extension coordinator transfers validated findings into preparation and owns scan-generation invalidation, without giving the detector direct database access. Pin the actual host choice after Josh's lifecycle integration is inspected.

Changing CNN enablement invalidates outstanding review tied to prior detection configuration. Existing committed mappings remain restorable whether inference is enabled or disabled. Include real CNN-produced PERSON/ADDRESS findings and rule findings in both vault backends' integration fixtures.

### 5. Qualify installed artifacts and shipping extension

Extend `cargo dfv` with explicit SQLite3MC preparation and check scopes, named during implementation. Preparation builds/downloads pinned dependencies and packages candidates; checks never install. Reports retain source commit, dirty input hashes where applicable, npm tarball/member hashes, SQLite3MC source/binary identity, cipher/VFS/parameters, browser/OS/hardware, CSP, input hashes, measurements, and every required case. Keep reports in timestamped retained directories. Missing/skipped cases fail the selected scope.

## Browser checks required before shipping

| Area | Required behavior and evidence |
| --- | --- |
| Installed packages | Clean consumer installs actual Core and adapter tarballs; TypeScript types compile; packaged Worker/WASM/CNN assets match expected hashes. No checkout-only imports. |
| Extension policy | Real MV3 origin, shipping bundler and CSP, packaged local assets, no external requests. Verify selected OPFS VFS and Worker work in actual extension host without an unsupported isolation requirement. |
| Reversible flow | Real rule and CNN findings -> exact review -> rejected approval -> approved encrypted save -> activation -> exact restoration. Manual selections and detector findings remain distinct evidence. |
| Persistence | Close/reopen page, Worker restart, MV3 service-worker suspension/restart, full browser restart, and actual extension upgrade preserve saved mappings and restore correctly. Reopening performs strict identity/head checks. |
| Encryption | Wrong password/key, altered pages/wrapping envelope, truncated DB, plaintext DB, wrong record/vault, malformed bundle, and incompatible schema fail closed with sanitized errors. Examine every persistent journal/temp/export artifact for the selected cipher profile. |
| Conflicts | Two tabs/owners compete on one workspace; exactly one expected-head save succeeds. Exercise browser file locks, stale owner, no-op save, vacant-record creation races, and copied input buffer mutation. |
| Interrupted save | Abort before commit, quota/storage denial, lost commit reply, Worker kill, page close, and browser termination near commit. On reopen observe a valid complete old or new candidate; never mixed heads/bundles. Uncertain outcomes grant no handoff. |
| Lifetime | Lock/close during unlock, preparation, commit, and activation boundary; late replies cannot activate, return plaintext, or authorize handoff. Revoked plans/streams reject reuse. |
| Recovery/migration | Password change interruption, recovery-credential restore, encrypted backup round trip, FogVault -> fresh SQLite3MC migration, occupied destination rejection; exact originals and aliases survive browser restart. |
| Conversation safety | Pending send remains pending across restart; only saved trusted observation advances it. No automatic resend or unscoped restore bypass. |
| CNN toggle | Default enabled after extension startup; disable prevents inference and releases inference resources according to its lifecycle. Both settings preserve saved restoration, stale review rejection and backend save sequencing. |
| Resource limits | Measure cold unlock/open, growing 1/100/1000-mapping bundles, prepare/save/reopen/restore, combined Core/CNN/DB memory and UI responsiveness. Include a representative lower-memory device. Reject admitted payload/KDF limits without partial publication. |
| Regression | Existing FogVault installed browser path, Workspace contract vectors, exact published beta.3 interchange, and detection offset parity remain passing. |

A local unanchored browser database cannot detect a fully replayed older valid database solely through its own encrypted revision fields. Document that limitation explicitly; do not claim new anti-rollback authority without an independently protected anchor. Browser storage eviction/user deletion is a recovery scenario; request available persistent storage and provide export, without treating the request as a guarantee.

Define measured budgets with Josh before implementation acceptance: supported browser versions, representative device, maximum admitted bundle, cold unlock, save/reopen latency, peak combined memory and longest UI stall. Evidence must report actual values rather than invented hardware-independent guarantees.

Automated Chromium extension tests are necessary but insufficient. Before release, Josh's actual packaged extension must execute enabled and disabled CNN flows through SQLite3MC on intended retail Chrome, and Edge/Firefox only if those are claimed supported. Record actual CSP, packaging, host lifetime, lock/reopen, real restart, and send-observation acceptance. Mocks and unpacked synthetic fixtures cannot substitute for this gate.

## Dependencies and release gates

Dependencies: pinned SQLite3MC build and selected persistent VFS; audited credential wrapping primitives and license/provenance; browser test harness and distributable npm packaging; actual extension source/host integration and Josh's acceptance; CNN public Rust/WASM detector result contract; fixture corpus and representative device.

Release gates: (1) OPFS/cipher/Worker proof; (2) reviewed persistence and credential ADR; (3) installed browser suite, fault cases and resource budgets passing; (4) joint CNN + both-backend extension flow passing; (5) shipping-extension acceptance; (6) existing exact-release-commit CI gates and unchanged supported regressions. Add one concise user-facing changelog entry for the final behavior. No test or implementation work was performed for this plan.

Run doctor first during implementation. For Rust changes, complete repository-required fmt, all-features clippy and all-features workspace tests. Detection changes also require `cargo dfv check`, positive/negative behavior fixtures, model contract checks and selected SDK parity. Use dedicated SQLite browser checks for actual backend behavior rather than treating generic Core tests as qualification.

## Risks and estimated complexity

HIGH: selected OPFS VFS durability/encryption behavior and extension support need direct proof; SQLite3MC is an encrypted database rather than a finished password/recovery vault, so credential and lifecycle work is substantial. HIGH: lost publication replies, browser termination and multi-owner conflicts can violate save-before-activation unless explicitly modeled. MEDIUM: large whole-bundle copies plus CNN and two WASM engines can exceed extension memory budgets; benchmark on representative hardware. MEDIUM: packaging/CSP and Worker host lifetime differ between harness and production extension.

Complexity: HIGH. Recommended delivery slices are storage proof/ADR, credential and binding contract, owner/CAS implementation, recovery/migration, installed fault suite, and shipping extension qualification. Work can coordinate with the CNN track after public finding/configuration and Worker ownership boundaries are agreed; end-to-end qualification depends on both.

## Confirmation

Implementation awaits approval of the combined v1.1 plan. This document contains planning only, as requested.

## Sources

- Existing Core contract: `docs/integrations/fogvault-contract.md` and `integrations/fogvault-browser/README.md`.
- SQLite3MC published browser builds and release notes: https://github.com/utelle/SQLite3MultipleCiphers/releases . These establish artifact availability, not our VFS/browser qualification.
- SQLite browser Worker/OPFS documentation: https://sqlite.org/wasm/doc/trunk/persistence.md . Apply only after confirming the selected SQLite3MC build retains the relevant VFS behavior.
