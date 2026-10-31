# Optional native Node Core–FogVault integration

`@datafog/core-fogvault-node` owns the Rust Core–FogVault adapter in a native
N-API addon. Version `1.0.0` implements the reviewed Core Workspace contract 1;
production host and key-provider acceptance remain separate.
Existing `@datafog/node` remains independent and retains its Rust 1.88 floor.
This optional isolated package builds with Rust 1.94.1 and uses Node 24+.

```sh
npm install --save-exact @datafog/core-fogvault-node@1.0.0
```

## Build and installed proof

From the Core repository root:

```sh
npm ci --ignore-scripts --prefix integrations/fogvault-node
node integrations/fogvault-node/test/run-installed.mjs --output target/fogvault-node-installed.json
```

That command builds a fresh release addon, runs Rust boundary tests, packs it,
installs the tarball into an isolated consumer without install scripts, compiles
positive/negative TypeScript usage, and executes the real installed addon against
synthetic SQLite/control namespaces. Source/tool/log, tarball and installed-addon
hashes are retained with the report. A local build packs one explicitly tagged
native binary and a checksum manifest. CI retains the actually qualified binary
from each existing runner (`ubuntu-24.04`, `macos-15`, `windows-2025`), checks their
source revision, public package files, installed tests and checksums, then assembles
one qualified archive. That archive is installed and exercised on
Linux without rebuilding its binaries. The assembly manifest records the exact
observed platform/architecture set, including the macOS runner's actual architecture.

The supported distribution scope is Linux x64 with GNU libc, Windows x64/MSVC,
and the macOS architecture actually provided and qualified by `macos-15` (arm64
or x64). This does not promise both macOS architectures in every archive. Linux
musl, Linux arm64 and all other unqualified targets fail with an informative
`unsupported_native_target` error; there is no download or source-build fallback.
A missing manifest, absent selected binary or checksum mismatch fails closed.
No registry package is published by assembly.
The package itself has no runtime npm dependencies. TypeScript is a pinned
repository-only development tool.

## Create, review, save and reopen

```js
import {randomBytes} from 'node:crypto';
import {NativeWorkspace} from '@datafog/core-fogvault-node';

const tenant = 'trusted-host-tenant-id';
const recordId = randomBytes(16), namespace = randomBytes(16);
const config = {
  tenant,
  protectedControlPath: '/host-established/protected/control',
  vaultPath: '/host-selected/vault.sqlite',
  password: Buffer.from('synthetic-example-only; obtain real bytes from host'),
  recordId, namespace, profile: 'password_only',
};
const owner = NativeWorkspace.createNew({
  ...config, operationId: randomBytes(16), requestId: randomBytes(16),
});
const text = 'alice@example.test';
const input = {
  text,
  scannerFindings: [{
    entityType: 'EMAIL', matchedText: text,
    range: {unit: 'utf8_bytes', start: 0, end: Buffer.byteLength(text)},
  }],
  userSelections: [], corrections: [],
};
const change = owner.prepareReview(tenant, [input], randomBytes(16));
const localPreview = change.previews(tenant); // Local review only.
// The host obtains literal user approval of this exact preview before continuing.
const committed = owner.commit(tenant, change, true, randomBytes(16));
// Only now may the host release committed.previews through its separate transport.
owner.close(tenant);
const reopened = NativeWorkspace.openExisting({...config, expectedHead: committed.head});
const restored = reopened.restore(tenant, committed.previews[0]);
reopened.close(tenant);
```

The control namespace must have real OS protection independent of the vault
files. A second path or directory alone is not isolation. Paths, tenant identity,
passwords, IDs and records must come from trusted host routing, not arbitrary
HTTP request fields. `createNew` requires a fresh control namespace and new vault;
`openExisting` never creates missing state. The default admission is 16 MiB Core
payload, 88 metadata bytes and 64 vault records; `maxPayloadBytes` can lower the
host's admitted payload ceiling. This does not establish a peak-memory budget.

Tenant-first calls check the exact captured tenant and creator thread. Those
checks catch host routing errors; they are not user authentication or filesystem
authorization. Keep an owner on its creating Node worker/isolate, serialize its
requests and reject stale/cancelled worker replies before network release. Native
KDF/storage calls are synchronous; this package does not move them to a worker
or provide global server admission. A cloned JS owner cannot become a new native
capability. No unsafe Send/Sync implementation is used.

IDs are exact byte values. Heads, receipt generations and record revisions are
lossless canonical decimal strings; do not convert them through Number. JSON
results carry ID arrays, which the host can copy into Uint8Array when needed.
Only ordinary copied buffers/plain DTOs are accepted. Proxies, accessors, shared
buffers, lossy numbers, malformed Unicode and forged/foreign handles reject.
Native-owned temporary password copies are wiped; host-owned input/output and
foreign copies remain the host's cleanup responsibility. Password bytes are not
normalized or serialized into JSON. Non-password access profiles reject with
`unsupported_profile`; the factory does not provide device fallback or hardware
claims.

## Lifecycle, restoration and recovery

Each owner admits one active prepared change. A second preparation returns
`resource_limit` before cloning Core state; read-only review remains available.
Discard, confirmed consumption, collection of an abandoned handle, or owner
lock/close releases admission. Keep explicit discard/close in application code;
garbage collection has no timing guarantee. This per-owner bound does not provide
aggregate server, model, stream or device memory admission.

`PreparedChange` and restore streams are opaque, owner-bound handles. The addon
uses the existing Rust Owner for all save, complete-head/record CAS, activation,
lock and uncertain-outcome authority. Raw Workspace/Vault handles, plaintext
bundle export and direct activation are not public exports. Local previews are
sensitive views, never a grant to bypass host approval.

Conversation preparation, observed-send and definite-non-delivery resolutions
use the same saved path. Only actual matching trusted host observation may claim
a send succeeded. Pending conversations remain blocked across reopen; unscoped
restore is not a fallback around scoped failure. Reports return null text when
marker issues block restoration. Strict streams buffer until finalization;
progressive streams are explicit irreversible output. Wrong feed transport is
terminated through Core's error path and retains prior progressive delivery
counts without failing-call text. Lock closes owned Core state/plans/streams;
caller copies cannot be retracted. Call `discard` on unused plans and `close` on
streams and owners for deterministic cleanup. Abandoned plan/stream wrappers
also release their native handles after JavaScript garbage collection; timing
is not guaranteed. A retained child keeps its owner alive, so dropping only the
owner variable is not a lock operation. Explicit lock/close always revokes all
children, including retained children.

Unknown-save errors retain `requestId` and `requiresReopen`. Keep the operation
identity in trusted host state. `owner.reconcile(tenant, requestId)` leaves Core
closed. After process loss, `NativeWorkspace.reconcileExisting` opens only the
registered locked vault and reconciles the exact request, returning a receipt
without plaintext or Core activation. Authenticate/open a fresh owner afterward;
never resend automatically or reuse an old approval. The dropped-ack installed
test discards an actual successful return; it is not a power-cut experiment.

`prepareRecovery` stages Core's same-vault recovery workflow. The separate
`importAuthenticatedBundle` factory accepts only a previously authenticated,
decrypted Core bundle with its exact source namespace/head, explicit approval
and a fresh destination. It delegates validation to the migration crate and
publication to the Rust Owner, creates fresh destination keys, and never
transplants legacy credentials. It does not decrypt legacy archives or establish
source authenticity itself. Final v1 schema migration remains a release gate.

## Evidence limits

Local installed tests do not qualify another OS, real key-provider hardware,
power loss, production tenant routing, deployment permissions, peak memory or
signed registry publication. Rust dependency and licensing/release review must
cover the final distributed binary. Root CI and joint release qualification are
separate. N-API pins match Core; the derive backend is constrained to Core's
6.1.2 version because a newer compatible-range backend moved its syn ABI beyond
pinned napi-derive 3.6.3.
