# Core–FogVault browser adapter candidate

This optional Core-owned package joins the maintained `@datafog/wasm` Workspace
to `@datafog/fogvault-browser`. It stores the exact Workspace bundle as an opaque
FogVault record, then activates only after a confirmed durable save. It contains
no encryption, key derivation, marker allocation or detection implementation.

The adapter and FogVault package remain unpublished candidates. Use locally
packed artifacts and exact dependency locks; this is not a v1 contract freeze or
production/security qualification. Core beta.3 stays the exact regression floor.

## Ownership and local review

```js
import {init, Workspace, scan} from '@datafog/wasm';
import {FogVault, randomId} from '@datafog/fogvault-browser';
import {FogVaultWorkspace} from '@datafog/core-fogvault-browser';

await init();
const vault = new FogVault({
  // Host admission choices, not a promise that these limits fit every device.
  limits: {payload: 16 * 1024 * 1024, metadata: 88, records: 64},
  storageProfile: 'arraybuffer',
  kdfProfiles: [{memory_kib: 65536, passes: 3}],
});
await vault.create('application-vault', passwordFromLocalUI);
const recordId = randomId(); // Persist this public identifier in trusted app routing.
const namespace = randomId();
let owner = await FogVaultWorkspace.create({Workspace, vault, recordId, namespace});

const text = 'Email alice@example.test';
const findings = scan(text).map(finding => ({
  entityType: finding.entityType, matchedText: finding.matchedText,
  range: {unit: 'utf8_bytes', ...finding.byteRange},
  detectorName: finding.detectorName,
}));
const plan = owner.prepareReview([{text, scannerFindings: findings, userSelections: []}], randomId());
showLocalReview(plan.preview()); // Local display is not approval or handoff.
// Only call this after the person approves this exact still-current review.
const saved = await owner.commit(plan, {approved: true});
showApprovedCopySurface(saved.previews[0].text);

await owner.lock(); // Core plans/streams become unusable immediately.
await vault.unlock(passwordFromLocalUI);
owner = await FogVaultWorkspace.open({Workspace, vault, recordId, expectedNamespace: namespace});
const restored = owner.restore(saved.previews[0].text);
await owner.close();
```

`showLocalReview`, `showApprovedCopySurface` and credential input are application
functions, not SDK exports. The supplied Workspace class and vault must be the
actual installed packages. The host initializes Core and establishes the vault's
credential/provider, Worker URL and finite resource policy.

Give the adapter exclusive use of the supplied vault owner after construction.
Do not mutate or lock that raw owner behind the adapter. The adapter does not
expose the raw Workspace, plaintext export, or raw prepared change. Its wrapped
plans expose the existing preview/heads/format/intents/discard operations. They
belong to exactly one owner. Reads and preparations are blocked during a save.
Discard unused plans and close streams as usual.

## Save failure, restart and transport

`commit` requires literal `true`, retains the exact Core parent/candidate heads,
and saves against both the accepted FogVault head and record revision. Only its
successful return grants permission to release the returned previews. It never
sends a network message, confirms delivery or retries a send.

A conflict, uncertain save, cancellation after submission or activation failure
closes the Core owner and grants no handoff. An `outcome_unknown` error retains
the exact FogVault request ID. Unlock/reopen the raw vault as needed and call its
`reconcileLatest(requestId)` before constructing a fresh adapter owner. Reopen
authenticated saved state and require fresh preparation and approval; never reuse
the old plan or infer that the save rolled back. Other error codes are preserved
without copying plaintext or provider details into new messages.

`lock()` and `close()` revoke Core synchronously before their returned Promise
awaits vault cleanup. Late mutation replies cannot activate a closed owner.
Returned preview/restoration strings and foreign copies remain the caller's
cleanup responsibility; locking cannot erase copies already returned.

Use `prepareConversationSend` for conversation-aware transport. Capture
`plan.sendIntents()` while the plan is live, save through `commit`, then hand off.
Only trusted observation of the exact send permits `prepareObservedSend` followed
by another approved save. A timeout remains pending and is never proof of success
or definite non-delivery. Scoped restore remains blocked while the conversation
has unresolved sends. Unscoped restore is not a fallback around scoped rejection.

## Binding and limits

Authenticated metadata uses the shared 88-byte `DFVBRG01` candidate contract in
[`docs/integrations/fogvault-contract.md`](../../docs/integrations/fogvault-contract.md).
Open checks record identity, namespace and the complete saved Core head against
the decoded bundle. Supply expected namespace/head when the application has
trusted expectations. This does not add anti-rollback authority to the browser's
unanchored offline profile.

Open expectations and request identities are copied before asynchronous work.
Caller buffer reuse cannot change an in-flight namespace/head expectation or
the operation identity used for persistence/reconciliation.

The initial adapter writes whole Core bundles. An indexed vault is not evidence
that these copies fit low-memory devices. I5 must qualify combined costs. Core's
existing Workspace/snapshot limits and the vault's separately admitted payload,
metadata and record limits all apply; no limit is silently increased.

`importAuthenticatedBundle` accepts only a separately authenticated/decrypted
Core bundle with mandatory trusted namespace/full-head expectations and literal
approval. It uses actual Core recovery to create a fresh head, requires an empty
destination vault under a fenced authenticated inventory, and binds the source
head as parent. Ordinary workspace creation can add a separate vacant record to
an existing vault; recovery import cannot merge into any occupied vault.
Even a repeated exact FogVault request ID cannot make occupied creation/import
succeed. Source credentials and wrappers are not transplanted. Caller-owned
source bundle bytes remain the caller's cleanup responsibility.

Legacy reference-host encrypted backups are not FogVault exports. This method
does not decrypt either encrypted file format or invent future v1 snapshot bytes.
Production legacy decryption and final migration approval remain separate I3 work.

## Installed-package proof

Build/package setup is separate from checks. After provisioning the repository's
pinned Core WASM/Playwright tools and both candidate tarballs:

```sh
node integrations/fogvault-browser/test/run-installed.mjs \
  --core-tarball target/fogvault-integration/artifacts/datafog-wasm-0.5.0-beta.3.tgz \
  --vault-tarball target/fogvault-integration/artifacts/datafog-fogvault-browser-0.0.0.tgz \
  --output target/fogvault-integration/browser-proof.json
```

The runner packs this adapter, installs all three candidates into an isolated
consumer, compiles their real TypeScript surfaces, compares installed assets to
tar members, and exercises the actual Core WASM + FogVault Worker/IndexedDB API.
Shared metadata unit tests cover binary vectors and malformed encodings. Fault
tests use the real browser suite, with narrow transport interception to exercise
reply loss and confirmed saves arriving after lock. They do not replace normal
actual package/Worker/IndexedDB execution.

The growth case records single-run 1/100/1000-mapping bundle sizes and separate
prepare/save/unlock/reopen/restore times, plus actual payload admission rejection.
These are advisory engineering samples. JavaScript heap counters exclude Worker,
WASM, allocator and foreign copies; they are not peak-memory or device budgets.

Default qualification is Chromium. Other engines must pass their own explicit
run; Chrome/Firefox/Edge SDK passes in FogVault alone do not prove this adapter.
Real shipping extension CSP/installation, physical-device budgets, providers,
security review and production acceptance remain separate qualification work.
