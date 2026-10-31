# Import supported Core reference backups

This optional `1.0.0` package reads only the non-shipping Core
workspace example's `datafog-workspace-backup` envelope versions 1 and 2. It
**does not establish compatibility with an unidentified shipping extension**.
Backup envelope versions are separate from the Core snapshot decoder versions.

```sh
npm install --save-exact @datafog/core-fogvault-legacy-browser@1.0.0
```

`LegacyBackupImport.open` authenticates the complete canonical archive, decrypts
it using the existing legacy algorithms, and validates its namespace/full head
through the initialized actual Core WASM decoder. The returned opaque session
exposes metadata/counts, not a plaintext bundle or legacy keys. `commit` requires
literal `approved: true`, fresh request/operation/record IDs and a fresh unlocked
FogVault destination. The existing browser adapter owns recovery, exact save,
activation, record/head CAS, occupied-destination rejection and reconciliation.
Source archive bytes are borrowed unchanged; only importer-owned copies are wiped.

```js
const staged = await LegacyBackupImport.open({Workspace, archive, credential,
  method: 'password', signal: controller.signal});
const metadata = staged.preview(); // Review locally before approval.
const owner = await staged.commit({vault: freshUnlockedVault, recordId,
  operationId, requestId, approved: true});
// owner is the maintained FogVaultWorkspace facade.
```

Password strings preserve the reference format's exact UTF-8 bytes, without
normalization; recovery codes use its existing DFR1 syntax. The original fixed
Argon2id v1.3/t3/p1 profiles remain 64 MiB (v1) and 256 MiB (v2), with no cheaper
fallback. Recovery uses the original HKDF-SHA-256/AES-GCM path. The importer uses
an actual disposable bundled libsodium worker and WebCrypto, not another crypto
implementation. `vault-read.mjs`, `backup-read.mjs` and the worker derive directly
from `examples/workspace-extension/src/{vault.js,backup.js,kdf-worker.js}`; only
read/authenticate helpers are retained. The original example remains the fixture
producer, so tests do not validate an importer against its own serializer.

Call `close` or abort the supplied signal to discard a session. Cancellation
terminates a pending KDF; after publication starts it locks the exclusive fresh
destination. A late persisted save may require receipt reconciliation and fresh
reopen; cancellation never promises rollback. Uncertain publication returns
`import_requires_reopen` with the exact request ID and `requiresReopen: true`.
A cleanup failure cannot hide that outcome: the error also carries
`cleanupFailed: true`; explicit close reports content-free `cleanup_failed`. Successful commit consumes the
session and removes its abort listener, so later cancellation does not lock the
returned owner. Serialize imports and destination creation/KDF work in the host.
JavaScript strings, CryptoKey cleanup and browser memory erasure are best effort.
The 24 MiB archive / 16 MiB bundle caps do not establish peak-memory qualification.

Build with `npm ci --legacy-peer-deps --ignore-scripts` and `npm run build`.
The installed runner accepts explicit Core and FogVault tarballs:

```
python scripts/verify-fogvault-legacy.py --prepared PREPARED.json --output REPORT.json --engine chromium
```

It packs this package plus the actual Core browser adapter, installs all four
packages in an isolated consumer, verifies installed bytes, runs TypeScript and
real browser/KDF/IndexedDB cases, and retains named evidence. Browser workers need
same-origin script loading and a CSP permitting WebAssembly (`wasm-unsafe-eval`).
The preparation marker must bind both consumed tarballs and the exact current
Core build inputs; a stale marker fails before the browser starts.
No RPC broker, synthetic fault control, legacy export API or credential-creation
API is included in the maintained importer package. Native legacy import and a
final v1 schema upgrade are separate scope; this package cannot close those gates.
