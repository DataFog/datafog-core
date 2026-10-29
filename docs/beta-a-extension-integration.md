# Beta A: Chrome extension integration contract

Target package: `@datafog/wasm@0.5.0-beta.1`. This is a developer integration
release for synthetic tests. The release candidate, package contents and final
verification evidence are tracked in [#132](https://github.com/DataFog/datafog-core/issues/132).
The version is a target until the publication checklist is complete.

Beta A supports detection, explicit local review, workspace pseudonyms, approved
handoff, strict restore, committed-marker follow-ups, atomic batches and encrypted
backup/recovery through the host reference workflow. Finding-source review and
recall are Beta B; remembered choices/rules are Beta C; conversation/provenance
records are Beta D. The final pilot snapshot/contract freeze is later. Beta A does not itself block
ChatGPT's send button; native composer input and uploads outside the protected
extension workflow remain outside this contract.

## Installation and session ownership

Pin the exact SDK version and lockfile. Updating the package does not activate
models or add the workspace UI to an extension. The current extension prototype's
rule-only scanner remains a separate flow.

Use an extension-owned page or side panel for the paste/review surface. The
page/tab can own a model and workspace session; durable encrypted mappings must
be rehydrated after its context closes or the browser restarts. A service worker's
in-memory variables are not durable storage. Cross-tab session sharing is not
required for V1. Treat model execution placement and UI responsiveness as explicit
host decisions; the SDK does not move inference into a worker automatically.

The synchronous `scan()`/`scanAndTransform()` exports remain rule-based. The
optional `@datafog/wasm/models` entry point provides the English GLiNER2.5 Small
model. Model setup is explicit and separate from the npm package:

- Provision the pinned ONNX graph and tokenizer files from
  [the browser model guide](browser-model.md), about 297 MB in total.
- Serve matching ONNX Runtime Web assets locally and configure `wasmPaths` when
  the bundler does not arrange them. Keep model/runtime licenses with the assets.
- Verify loading in an actual Manifest V3 extension with its deployed CSP, asset
  URLs and bundler output. A normal web page is a different execution context.
- Use one thread without cross-origin isolation. Record first-session local load,
  inference, UI stalls and memory; local asset loading does not include a network
  download. Successful development hardware tests are not low-memory guarantees.

## Detection to a workspace preview

```js
import {init, Workspace} from "@datafog/wasm";
import {ModelSession} from "@datafog/wasm/models";

const randomId = () => crypto.getRandomValues(new Uint8Array(16));
await init();
const workspace = Workspace.create(randomId(), randomId());
const model = await ModelSession.open({
  modelBaseUrl: new URL("./models/", location.href),
  wasmPaths: new URL("./ort/", location.href).href,
  numThreads: 1,
});
const source = "Contact John Smith at john@example.com.";
const result = await model.scan(source, {entities: ["PERSON", "EMAIL"]});
const change = workspace.prepare(source, result.findings.map(finding => ({
  entityType: finding.entityType,
  matchedText: finding.matchedText,
  range: {unit: "utf8_bytes", ...finding.byteRange},
  confidence: finding.confidence,
  detectorName: finding.detectorName,
  detectorVersion: finding.detectorVersion,
})), randomId());
const preview = change.preview(); // Show locally; do not hand off yet.
```

`model.scan()` and `scanModel()` are async. `scan()` routes supported types such
as EMAIL/PHONE through rules and the remaining configured types through Small;
`scanModel()` requests model-only output. `describe()` returns configured metadata,
not a readiness or accuracy claim. Small's default `ADDRESS` is a full address
span; it is different from the street-line `STREET_ADDRESS` type.

All ranges are zero-based and end-exclusive. Workspace findings take UTF-8 byte
ranges; browser editor selections and highlights normally use UTF-16 ranges.
Do not use byte offsets as JavaScript string indexes. `workspaceByteRangeFromUtf16`
converts a user selection through Core. Each `prepareBatch` member uses its own
text-local ranges, shares the resulting mappings and needs one whole-batch approval.

A model request is bounded to 65,536 UTF-8 bytes and 512 tokens including its
schema. Text is never silently truncated. The host must explicitly split larger
input before scanning, preserving the local offsets when preparing a batch.
There is one inference at a time. Busy, malformed output, missing/corrupt assets,
input bounds and cleanup failures are explicit errors, not successful rule-only
fallbacks. There is no inference cancellation/deadline or automatic chunking.

## Source marker blocking and retry

`prepare`/`prepareBatch` reject damaged or unknown marker-like text and bare
committed codes before returning a plan. A source such as `[[my_note]]` must be
corrected before approval. Full committed markers pass through unchanged;
ordinary text such as `[[my note]]` stays ordinary text.

An `invalid_marker` error exposes `markerIssues`, with `kind`, `byteRange`,
`codepointRange` and `utf16Range`. These are content-free ranges; the UI renders
highlights from its own local source. `inputIndex` locates the rejected batch
member, while finding indexes are local to that member. A rejected batch returns
no partial plan and changes no committed mappings. Edit, discard old approval
and prepare a fresh preview; do not add an unknown-marker passthrough exception.

## Approve, encrypt, durably save, then activate

The SDK owns marker identity and snapshot semantics. The host owns credentials,
encryption, storage, user approval and handoff. The vault/broker/storage/backup
files in [the reference harness](../examples/workspace-extension/README.md) show
this flow with real IndexedDB and cryptography. They are reference host code,
not SDK exports; do not copy the test-only fault controls into a shipping extension.

On an explicit approval, the host must:

1. Confirm the exact source, findings, batch membership and current generation
   still match the preview the person approved.
2. Read `change.heads()` and export the candidate bundle for the trusted encrypting
   host only. The revision is a decimal string, not a JavaScript number; compare
   the complete head (revision and commit ID).
3. Encrypt the candidate with the current workspace key. In one IndexedDB
   read/compare/write transaction, require the expected saved head and vault
   access generation, then persist the new encrypted record. Wait for transaction
   completion, not just a successful write request.
4. Recheck approval/cancellation state and call
   `workspace.activateSaved(change, expectedHead, candidateHead)` only after
   confirmed persistence of that exact candidate.
5. Release the approved preview to the copy/insert handoff surface only after
   activation succeeds. Clear temporary source and candidate bytes.

An abort, conflict, lock, edit, lost save acknowledgment or stale approval must
release no new text. Do not automatically retry a handoff after an uncertain save;
reopen from durable encrypted state and require fresh review. New operations use
fresh host-random 16-byte IDs; never derive them from source text or use Math.random.

Successful activation consumes its plan. WASM also drops retained plans whose
parent head has become stale; those handles return `inactive_plan`. No-op and
failed activations preserve valid current-head plans. Explicitly discard unused
live plans; the 32-live-plan bound remains.

## Restore, follow-ups and recovery

`workspace.restoreReport(answer)` returns text only when every marker resolves.
Otherwise `text` is null and `issues` lists unknown, damaged or unbracketed marker
ranges for local correction. `restore()` is the strict throwing wrapper. Never
show a partial restored answer or a fallback that mixes real values and unresolved
markers.

A follow-up can contain full markers already committed in this workspace. They
pass through unchanged; findings must cover only new sensitive spans outside
those markers. Every follow-up still needs a new preview and approval, even when
its mapping head does not change. A marker allocated in an unsaved plan is not a
committed reference, including in another member of the same pending batch.

The host reference workflow exports authenticated encrypted backups with the
export-time password/recovery wrappers. Recovery needs explicit local approval
and an empty destination; it never overwrites or merges an occupied workspace.
`prepareRecovery` stages a fresh saved head and grants no text-handoff permission.
Aliases issued after an older backup remain unknown after its restoration.

## Close and reopen

On lock or an invalidating edit, stop granting handoff permission immediately,
clear temporary views and discard the affected plans. `workspace.close()`
invalidates that runtime's plans. Reopen with `Workspace.openBundle()` only after
the host decrypts and validates the saved vault record, then clear the temporary
bundle bytes.

`await model.close()` is idempotent after success and rejects with
`model_session_busy` during inference. Handle that result explicitly; ignore late
results after an approval-generation change, then close when inference finishes,
or destroy the owning context if a hard stop is required. Open a new model session
explicitly when needed. A model session is not persisted with the encrypted vault.

## Beta and pilot boundaries

Pin the exact Beta A package. Snapshot schema 1 and the workspace contract remain
provisional through the later V1 work; recreate disposable development vaults on
upgrades unless a migration has been explicitly qualified. Do not promise existing
real-data vault compatibility from the beta version alone.

Use synthetic values for the integration acceptance sequence: detect → preview →
approve → save → handoff → restore → follow-up → lock/restart → reopen → backup
and recovery. Include malformed markers, failed saves, conflicting heads, stale
plans and missing assets. The same sequence must run against the published package
after publication, with exact Core and extension commits recorded.

A real-data pilot additionally requires the intended snapshot/contract freeze,
marker preservation tests with model providers, correctly scoped detection
qualification, complete backup-key coverage and vault/backup encryption review.
Native GLiNER2 scores and an accepted prompt label do not qualify Small accuracy.
English model misses, partial addresses and false positives still require review.
