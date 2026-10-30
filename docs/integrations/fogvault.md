# Using Core with FogVault

Core now has optional native Rust, Node, Python and browser integration
candidates. They keep Core preparation separate from persistence and activate only after the exact
candidate has been saved. Core's existing 0.5.0-beta.3 packages and Rust 1.88
minimum are unchanged. These new integration packages remain unpublished 0.0.0
candidates; this change does not freeze the v1 API or approve a production host.

## Start here

- Browser: [typed owner, runnable example and installed-package proof](../../integrations/fogvault-browser/README.md).
- Native Node: [installed addon, tenant boundaries and lifecycle](../../integrations/fogvault-node/README.md).
- Native Python: [installed wheel, thread boundaries and lifecycle](../../integrations/fogvault-python/README.md).
- Native Rust: [registered SQLite owner and recovery](../../integrations/fogvault-native/README.md).
- Legacy backups: [authenticated reference-format importer](../../integrations/fogvault-legacy-browser/README.md).
- Migration: [authenticated/decrypted bundle recovery staging](../../integrations/fogvault-migration/README.md).
- Shared [record and save/lifetime contract](fogvault-contract.md).
- [Compatibility, backup differences and open-issue reconciliation](fogvault-compatibility.md).

The adapter owns the Core Workspace and its plans/streams. The application owns
literal approval of the current review, authenticated tenant/routing context,
credential input and transport. FogVault owns encryption and authenticated
publication. Raw source text never needs to enter the chat site's page to use
this integration. Preparation is local review, not permission to copy or send.

## Prepare and verify

Run the existing environment setup, then build Core's bindings and inspect the
repository-sourced prerequisites:

```sh
cargo dfv build
cargo dfv doctor
cargo dfv build --fogvault
cargo dfv check --fogvault
```

The explicit FogVault preparation command checks out exact public revision
`6c6176a3a9df320447ba42b924b26de7ac4d693a` into Core's ignored
`target/fogvault-integration/dependencies/`. It installs that candidate's locked
build tools, provisions Rust 1.94.1's WASM target, builds/packages it there and
packs Core's current WASM package. It does not modify an existing FogVault
checkout. Build logs, package hashes, pinned source identity and current dirty
Core build/package input hashes are retained in `target/fogvault-integration/`.

Checking does not install dependencies. It requires the prepared package bytes
and Core build inputs still to match, runs named required Rust and actual
installed browser cases, and rejects omitted/skipped/malformed results. Each
run gets a fresh directory with logs, hashes and source identity. Do not reuse
old passing evidence after changing source or package bytes.

`cargo dfv check --fogvault --native-only` selects Rust migration/native tests;
`--browser-only` selects the installed-browser and binding checks. The dfv
entry point still uses Core's full environment diagnosis. CI may invoke the same
Python gate directly for a Rust-only host with its explicitly installed native
prerequisites. A native subset is not browser qualification.

The native integration has its own Rust 1.94.1 qualification toolchain and a
tested minimum of Rust 1.89. Core itself stays on 1.88. Optional native Node and
Python packages wrap that same Rust owner. They are separate from the existing
Core SDKs, support password-only admission, and retain literal approval,
complete-head save, lock/reopen and uncertain-outcome rules. Tenant/thread checks
catch host routing errors; the application still owns authentication, path
permissions and an independently protected native control namespace.

Prepare and check the native language packages separately:

```sh
cargo dfv build --fogvault-languages
cargo dfv check --fogvault-languages
```

This selected scope diagnoses its own pinned native build tools and does not
require unrelated original SDK outputs. Setup installs the declared Node build
tools; checking never installs dependencies. It builds an actual npm tarball and
Python wheel, installs each in a fresh consumer, runs named behavioral cases and
type checks, then runs Node → Python → Node against the same registered SQLite
vault and control namespace. Logs, package identities, source hashes and reports
are retained in `target/fogvault-language/check-*/`. CI qualifies this installed
path on Linux, macOS and Windows. It does not qualify production tenant routing,
physical providers, power cuts or every Python/Node version.

For a future Core v1 release, each existing publish workflow also requires both
joint CI workflows to have succeeded for the exact candidate commit, including
all six native/browser scopes and all three native-language OS scopes. Pending,
failed, incomplete or older-commit evidence blocks publication. This gate does
not publish packages or waive the remaining release approvals.

Both `--fogvault` and `--fogvault-languages` are required joint qualification
scopes for changes that affect all maintained integrations. Neither publishes
packages or changes the existing Core beta.3 minimum. The optional packages stay
unpublished `0.0.0` candidates until the remaining v1 gates close.

## Browser extension handoff

1. Pin the exact Core, FogVault and integration artifacts. Initialize Core;
   configure the FogVault Worker URL, finite resource/KDF policy and credentials
   in an extension-owned trusted page. A service worker's memory is not storage.
2. Give the adapter exclusive ownership of the supplied handles. Persist public
   routing identifiers so reopen selects the intended workspace/record.
3. Prepare and display the local review. Editing, locking or changing ownership
   invalidates permission to hand off the old result. Commit only after explicit
   approval of that exact still-current plan.
4. Hand off only the result of a confirmed save followed by Core activation.
   This does not establish remote delivery. Conversation intents must remain
   pending until the host saves a trusted observation of the actual send.
5. On an uncertain save, retain the exact request ID, reconcile/reopen durable
   authenticated state and prepare/approve fresh work. Never infer rollback,
   reuse an old approval, fall back to unscoped restore or automatically resend.
6. Lock through the adapter so Core is revoked immediately, before asynchronous
   vault cleanup. Clear application-owned source/preview copies separately.

The runnable browser example is integration code, not a completed shipping
extension. Josh's actual CSP/bundler/host send-observation acceptance remains
Core issue #132 and gate G4. Browser tests use actual installed Core WASM,
FogVault Worker and IndexedDB; synthetic host observations do not prove real
chat-site identity or network delivery.

## Remaining v1 gates

CSV is owner-deferred from v1. Existing marker/detection qualification (G1/G2),
complete format/API freeze (G3), exact production-host/provider/security review
(G4), shipping-extension acceptance and registry release remain separate. The
open-issue assessment identifies additional native-model fault and current-model
false-positive dispositions rather than declaring them closed.

Recovery staging validates separately authenticated/decrypted source bundles and
uses Core recovery; it does not decrypt legacy encrypted backup files or invent
future v1 schema bytes. [Resource observations](fogvault-resource-budget.md)
include isolated native process peak RSS and support the integrated owners'
one-active-preparation limit. Combined browser memory, installed-language copy
costs, device floors, real power loss and every OS/browser remain unqualified. Resolve the
documented FogVault registry dependency-distribution blockers before publishing
both products as 1.0.0.

The browser portion of `cargo dfv check --fogvault --engine ENGINE` also requires
an installed import-only reader for the supported **reference** encrypted backup
v1/v2 envelopes. `cargo dfv build --fogvault` explicitly prepares its pinned worker
build dependencies. The check authenticates actual legacy files using both
password and recovery paths, validates Core identity/head, and publishes only
approved recovery into a fresh FogVault destination. It tests real cancellation,
uncertain publication with cleanup failure, and fresh reconciliation/reopen.
Named evidence is retained as `legacy.json` beside `browser.json`.

This covers the non-shipping reference-host format; it does not establish the
archive provenance or format of Josh's shipping extension. Final v1 schema
compatibility and production security review remain open gates. See the
[reader scope and resource limits](../../integrations/fogvault-legacy-browser/README.md).
