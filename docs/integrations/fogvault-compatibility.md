# Core–FogVault compatibility and release boundaries

This work targets the Core-owned optional integration. The inspected baselines
are Core [`1ffd46a`](https://github.com/DataFog/datafog-core/tree/1ffd46accb7132083e562e1048ced951fd642c65)
and FogVault [`6c6176a`](https://github.com/DataFog/fogvault/tree/6c6176a3a9df320447ba42b924b26de7ac4d693a).
Both products target a later public 1.0.0 release; these integration packages
remain unpublished candidates. CSV is deferred from v1 by the owner decision on
October 30, 2026. No publication or outside-Core source changes are part of this
work.

## Three independent versions

Do not equate the Core package version, inner Workspace snapshot version and
outer storage/backup format. Core `0.5.0-beta.3` uses the current snapshot v3 and
also reads supported v1/v2 snapshots. The new `DFVBRG01` record-binding version
belongs to the integration, not to Core snapshot v1 or package 1.0.0. A future
package 1.0.0 does not imply a snapshot version change unless G3 approves one.

The exact published Core `=0.5.0-beta.3` regression must stay separately verified.
Testing working-tree Core prepares the v1 candidate; it does not prove that the
published beta package is interchangeable with future source. Run the joint
consumer suite with the actual selected package and artifact identities.

## Import formats are different contracts

| Input | Existing authority | Integration treatment |
| --- | --- | --- |
| Core Workspace bundle | Canonical Core CBOR; contains plaintext values. It has no encryption or archive-origin proof by itself. | Decode through Core and check the authenticated source namespace and full saved head. Never expose it as a safe public backup. |
| Core reference-host backup | Canonical JSON `datafog-workspace-backup`, supported outer versions 1/2; authenticated AES-GCM content and password/recovery wrappers. Outer version/profile is separate from the inner snapshot version. | Authenticate and decrypt the entire archive through a separately reviewed legacy importer before calling bundle recovery staging. Do not copy synthetic harness fault hooks or make test crypto a shipping dependency. |
| FogVault age/FVEXP001 export | Decoded-record snapshot encrypted to an independent age X25519 recipient. It does not carry original source password/device/recovery wrappers. Origin policy is explicit. | Use the FogVault importer and its framing, finality, resource and origin rules. Then validate any Core record through the bridge binding. Do not pass this archive to the legacy Core decoder or transplant credential wrappers. |

Source references: Core [backup authentication](https://github.com/DataFog/datafog-core/blob/1ffd46accb7132083e562e1048ced951fd642c65/examples/workspace-extension/src/backup.js),
[reference recovery sequencing](https://github.com/DataFog/datafog-core/blob/1ffd46accb7132083e562e1048ced951fd642c65/examples/workspace-extension/src/broker.js),
and FogVault [backup/data-transfer contract](https://github.com/DataFog/fogvault/blob/6c6176a3a9df320447ba42b924b26de7ac4d693a/docs/maintained.md).

## What the Core-only recovery helper does

[`integrations/fogvault-migration`](../../integrations/fogvault-migration/README.md)
accepts a separately authenticated/decrypted bundle and expected source identity.
It checks the namespace **and both head fields**, prepares an explicit fresh-ID
Core recovery, and reopens the resulting candidate through the public decoder.
It retains no caller source mutation and adds no alternate serializer. Candidate
plaintext is held in a zeroizing buffer; external copies remain caller-owned.

Existing synthetic v1/v2/v3 fixtures check alias and policy preservation, old
history retention, pending/resolved conversation authority, high u64 revisions,
and controlled rejection of malformed/future snapshots or exhausted/reused
operation authority. A recovery adds its own operation/head; it must not change
an unresolved send into a confirmed delivery or grant text handoff permission.

The maintained destination integration must separately require literal recovery
approval, create fresh vault identity/keys, atomically reject occupied
destinations, publish the exact bound candidate and reopen it. Unknown outcomes
require exact-operation reconciliation, not automatic replay. Keep the source
archive until destination reopen is confirmed. Do not equate process-crash tests
with physical power-loss qualification.

This helper closes the source-validation/staging portion of I3. It does **not**
close a production legacy decryption/import path, destination publication proof,
or migration to future v1 bytes that have not been specified. Those remain I3/G3
acceptance work. Full-snapshot memory budgets, including source/candidate/Core
state copies, belong in combined I5 measurement.

## Public distribution feasibility

| Component | Inspected state | Core-side action and remaining boundary |
| --- | --- | --- |
| Core Rust/Python/Node/WASM | Published beta.3; existing release checks normalize Cargo/npm/PEP 440 versions and verify tag consistency. | Preserve the beta regression, add joint candidate checks to `cargo dfv`, and prove actual installed v1 candidates before promotion. A version bump alone is not acceptance. |
| Optional native adapter | FogVault native currently requires Rust 1.89; Core's declared floor is 1.88. | Use an isolated integration toolchain and declare the higher adapter floor explicitly. Do not silently raise Core's global minimum. Exact Git/source consumption can prove integration before registry publication. |
| FogVault Rust packages | `fogvault`, `fogvault-native` and `fogvault-backup` are `0.0.0` with `publish = false`. Native backup is enabled by default. | A Core adapter can use a pinned Git/source candidate. Publishing FogVault packages requires FogVault-owned changes and separate user discussion before edits outside Core. |
| FogVault backup graph | age `0.11.5` uses localized, provenance-retained dependency corrections. Registry publication does not preserve those local path corrections under unchanged upstream names. | Preserve the corrected graph during source testing. Upstream fixes or separately named/published fork packages must resolve this before claiming a registry release of that graph. Do not silently substitute registry age as an apparent fix. |
| FogVault browser package | `@datafog/fogvault-browser` is private `0.0.0`; tarball includes Worker/WASM and has no runtime npm dependencies. | Install its actual packed source candidate for qualification. A public npm release requires FogVault-owned version/publish/distribution changes. The Core adapter must not download an implicit latest SDK. |

Sources: FogVault [native manifest](https://github.com/DataFog/fogvault/blob/6c6176a3a9df320447ba42b924b26de7ac4d693a/crates/fogvault-native/Cargo.toml),
[backup manifest](https://github.com/DataFog/fogvault/blob/6c6176a3a9df320447ba42b924b26de7ac4d693a/crates/fogvault-backup/Cargo.toml),
[browser manifest](https://github.com/DataFog/fogvault/blob/6c6176a3a9df320447ba42b924b26de7ac4d693a/packages/browser/package.json),
[dependency scope](https://github.com/DataFog/fogvault/blob/6c6176a3a9df320447ba42b924b26de7ac4d693a/docs/dependency-scope.md),
and Core [release version check](../../scripts/check-release.py).

I6 remains open until both exact candidate versions have compatible dependency
graphs, clean install/dry-run and joint-verifier evidence, approved release notes,
and the required G1–G4 acceptance. After authorized publication, test actual
registry packages and record package hashes/versions. Independent implementation
review, real key/provider and shipping-host acceptance remain explicit gates;
synthetic provider tests do not substitute for them.

## Open Core issue reconciliation

On October 30, 2026, the GitHub open-issue query returned **20 issues**. The
table below accounts for each one. This is a scope/release reconciliation, not
an assertion that unimplemented backlog features have been approved for v1.
Only the CSV deferral below is a new owner-approved disposition; other proposed
deferrals need the release owner to ratify the supported v1 feature set. Actual
failures on an advertised launch path still block its qualification.

| Open issue(s) | Disposition for this work | Coverage or required next action |
| --- | --- | --- |
| [#150](https://github.com/DataFog/datafog-core/issues/150), [#151](https://github.com/DataFog/datafog-core/issues/151) | Existing required launch gates G1/G2. | Real provider-marker and shipping-browser detection/recall acceptance remain open. Neither source tests nor the adapter suite substitutes for those results. |
| [#153](https://github.com/DataFog/datafog-core/issues/153), [#154](https://github.com/DataFog/datafog-core/issues/154) | Existing required launch gates G4/G3. | Exact shipping host/provider/security acceptance and permanent contract freeze remain open; I1–I7 supply integration inputs. |
| [#132](https://github.com/DataFog/datafog-core/issues/132) | Shipping-extension acceptance, represented by I7/G4. | Published Core SDKs and reference hosts do not prove Josh's extension adopted the package or save/scoped-send flow. Changes in the extension repository require the user's outside-Core discussion first. |
| [#149](https://github.com/DataFog/datafog-core/issues/149) | **Owner deferred CSV from v1 on October 30, 2026.** | Record the scope disposition in the tracker. Do not mark an unimplemented CSV feature Done. No CSV implementation is authorized in this integration pass. |
| [#90](https://github.com/DataFog/datafog-core/issues/90) | Required disposition under G4/I5 for any advertised native model path; not covered by a vault-only green suite. | Actual ModelSession worker OOM/exit, malformed-output, cancellation/timeout and retirement evidence remains a distinct native qualification issue. Qualify the supported CPU configuration; GPU-only conditions cannot become a claimed pass without a supported GPU path. |
| [#85](https://github.com/DataFog/datafog-core/issues/85) | Requires current-candidate reproduction and G2 disposition if the affected GLiNER2 model is in the advertised v1 path. | The issue reproduces a pronoun false positive on an older published model session. It does not establish that the current browser Small export or current native candidate has the same defect. Reproduce on the exact supported package/model/configuration before choosing a narrow fix or documented limitation. |
| [#88](https://github.com/DataFog/datafog-core/issues/88), [#89](https://github.com/DataFog/datafog-core/issues/89) | Proposed deferred new authorization/context APIs. | Existing host review/explicit exact choices remain the supported boundary. Do not add blanket finding exclusions or claim general trusted-field/public-number preservation. If a v1 consumer needs these new promises, review the API and include its qualification before freeze. |
| [#124](https://github.com/DataFog/datafog-core/issues/124), [#125](https://github.com/DataFog/datafog-core/issues/125), [#126](https://github.com/DataFog/datafog-core/issues/126) | Proposed deferred shared-engine architecture/routing expansion. | These are foundations for the separately proposed contributed engine, not a prerequisite demonstrated by the existing Core–FogVault storage bridge. Preserve current routing and asset-verification contracts; a failure of those existing contracts still blocks. |
| [#113](https://github.com/DataFog/datafog-core/issues/113), [#127](https://github.com/DataFog/datafog-core/issues/127) | Proposed deferred Decima native/browser engine expansion. | Do not promise added multilingual model coverage at v1 without separately approved integration and exact-export qualification. |
| [#80](https://github.com/DataFog/datafog-core/issues/80), [#86](https://github.com/DataFog/datafog-core/issues/86) | Proposed deferred native model batching and CUDA features. | Preserve current single-document/CPU behavior and explicit limits. Neither new capability is needed to prove the vault adapter; CUDA absence must not be represented as device qualification. |
| [#49](https://github.com/DataFog/datafog-core/issues/49) | Proposed deferred detector metadata redesign. | Current supported phone behavior must still meet G2 and existing regression checks. Broader numbering-plan semantics need separate compatibility and recall decisions. |
| [#50](https://github.com/DataFog/datafog-core/issues/50) | Nonblocking CSV/TSV performance follow-up; outside deferred CSV scope. | Its own issue describes the follow-up as nonblocking. It is not authority for large-file support or a new launch dependency. |
| [#165](https://github.com/DataFog/datafog-core/issues/165) | Already explicitly post-launch investigation. | Keep full-history preservation and finite capacity errors; no silent record deletion or compaction. |

The two qualification dispositions (#90 and #85) must not disappear behind the
seven new integration rows. Record their accepted scope under G4/G2 or as linked
release checklist entries. G1–G4 still require their real external/host evidence;
the integrations implemented here do not close all 20 issues or certify a fully
qualified public release.

## Supported reference encrypted archives

The optional `fogvault-legacy-browser` reader covers the Core example's
`datafog-workspace-backup` envelopes v1/profile1 and v2/profile2, with both
password and recovery-code authentication. These envelope versions are separate
from supported Core snapshot decoder versions. It preserves the source archive,
authenticates unused credential slots and complete metadata, and delegates fresh
destination recovery/publication to the maintained browser adapter. No legacy
credential slots or encryption keys are transplanted.

`cargo dfv check --fogvault` requires its eight named installed-browser cases and
all four envelope/credential combinations on the selected browser, in addition
to existing native and browser checks. Native-only checks require no JavaScript
importer dependencies. The example is explicitly non-shipping: compatibility
with the actual production extension's archives still requires identified source
artifacts. Final G3/v1 format migration and independent shipping security approval
remain open; reference-format test results do not close them.
