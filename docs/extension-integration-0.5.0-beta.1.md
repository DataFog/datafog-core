# 0.5.0-beta.1: Chrome extension integration contract

Target package: `@datafog/wasm@0.5.0-beta.1`. This is a developer integration
release for synthetic tests. The release candidate, package contents and final
verification evidence are tracked in [#132](https://github.com/DataFog/datafog-core/issues/132).
The version is a target until the publication checklist is complete.

Release scope and future version assignments come from the
[requirements spreadsheet](https://docs.google.com/spreadsheets/d/1Z5bwpzXiyncgg4nC5tPehSoO3mvg6nTGh8gxI4Q-3y0/edit).
The [public-call reference](#complete-public-api-reference) and
[error reference](#errors-and-retry) below cover this package version. The
workspace API and snapshot format remain provisional.

`0.5.0-beta.1` supports detection, explicit local review, workspace pseudonyms, approved
handoff, strict restore, committed-marker follow-ups, atomic batches and encrypted
backup/recovery through the host reference workflow. Finding-source review and
recall target `0.5.0-beta.2`. Later rules/choices and conversation/provenance work
follow the versions or TBD assignments in the requirements spreadsheet. The final
pilot snapshot/contract freeze is later. `0.5.0-beta.1` does not itself block
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

## Complete public API reference

This inventory covers every public function and session/plan method declared in
[`@datafog/wasm`](../bindings/wasm/index.d.ts) and
[`@datafog/wasm/models`](../bindings/wasm/models/index.d.ts) for `0.5.0-beta.1`.
Type names below refer to those declarations. Workspace calls are synchronous;
model operations are asynchronous except `describe()`. Host encryption, storage,
approval, clipboard insertion and backup UI are separate host responsibilities.

### Initialize and use the stateless root API

Import these calls from `@datafog/wasm`. Call `await init()` before a root scan,
transform, workspace factory or selection conversion. `ModelSession.open()` also
awaits initialization internally. An initialization failure must stop the flow.

| Call | Returns | Role and constraints |
| --- | --- | --- |
| `init()` | `Promise<void>` | Initialize Core WASM once before using root operations. Importing the optional model entry point does not load model assets. |
| `scan(text: string, config?: ScanConfig)` | `Finding[]` | Rule-based detection; no implicit model inference. Findings include byte, codepoint and UTF-16 ranges. |
| `transform(text: string, findings: FindingInput[], config: TransformationConfig)` | `TransformResult` | Apply supplied findings and return text plus transformation ranges. Use `Workspace.prepare` for persistent workspace pseudonyms. |
| `scanAndTransform(text: string, config: ScanAndTransformConfig)` | `TransformResult` | Stateless rule scan followed by transformation. It does not create or commit a workspace. |
| `restore(text: string, context: PrivacyContext)` | `RestoreResult` | The older provider-backed stateless API; browser WASM rejects it with `unsupported_strategy` after validation. Use `workspace.restore` for workspace markers. |
| `discoverFields(data: JsonDocument, config?: StructuredScanConfig)` | `FieldMapping[]` | Discover PERSON fields in structured JSON; it does not implement CSV import. |
| `scanStructured(data: JsonDocument, config?: StructuredScanConfig)` | `StructuredScanResult` | Rule/field-based findings with JSON Pointer paths and ranges local to each decoded string. |
| `transformStructured(data: JsonDocument, findings: StructuredFindingInput[], config: TransformationConfig)` | `StructuredTransformResult` | Transform supplied findings in JSON leaves atomically. |
| `scanAndTransformStructured(data: JsonDocument, config: StructuredScanAndTransformConfig)` | `StructuredTransformResult` | Stateless structured scan and transformation; not a workspace batch. |
| `restoreStructured(data: JsonDocument, context: PrivacyContext)` | `never` | Provider-backed structured restoration is unsupported in browser WASM. |

For stateless transformations, browsers support `redact`, `mask` and `remove`.
The shared declarations include provider-related types/codes for cross-SDK
consistency; they do not enable key/token providers in WASM.

### Workspace and selection calls

Import `Workspace` and `workspaceByteRangeFromUtf16` from `@datafog/wasm`.
Construct workspaces only through the two factories; pending plans are opaque
objects returned by Core, never caller-constructed or transferable handles.

| Call | Returns | Lifecycle role |
| --- | --- | --- |
| `Workspace.create(namespace: Uint8Array, operationId: Uint8Array)` | `Workspace` | Create an in-memory workspace using two host-random 16-byte public IDs. Creating it does not save a vault. |
| `Workspace.openBundle(bytes: Uint8Array)` | `Workspace` | Validate and reopen a plaintext Core bundle after trusted host decryption. Clear temporary bytes afterward. |
| `workspace.head()` | `WorkspaceHead` | Read the complete currently committed head for persistence/conflict handling. |
| `workspace.namespace()` | `Uint8Array` | Read the 16-byte public workspace namespace. It is not a credential or a marker to send to a model. |
| `workspace.exportBundle()` | `Uint8Array` | Export the current committed plaintext bundle for trusted host encryption; does not include uncommitted plans. |
| `workspace.prepare(text: string, findings: readonly WorkspaceFinding[], operationId: Uint8Array)` | `PreparedWorkspaceChange` | Validate source/findings and stage one preview/candidate without changing committed mappings. |
| `workspace.prepareBatch(inputs: readonly WorkspaceInput[], operationId: Uint8Array)` | `PreparedWorkspaceChange` | Stage 1–32 inputs with shared aliases and one candidate. A rejected member blocks the whole batch. |
| `workspace.prepareRecovery(operationId: Uint8Array)` | `PreparedWorkspaceChange` | Stage a new recovery head while preserving known aliases. It has no text preview or handoff permission; save and activate through the host's recovery flow. |
| `workspace.activateSaved(change: PreparedWorkspaceChange, expectedHead: WorkspaceHead, savedHead: WorkspaceHead)` | `void` | Adopt this runtime's exact pending candidate only after confirmed durable storage. `expectedHead` and `savedHead` must match the plan's expected/candidate heads. Success consumes it and reclaims plans with stale parent heads. |
| `workspace.restore(text: string)` | `string` | Strict all-or-nothing marker restoration. Blocking marker issues throw `invalid_marker`. |
| `workspace.restoreReport(text: string)` | `WorkspaceRestoreReport` | Return `{text, restoredMarkers, issues}`. Blocking markers produce `text: null` and issue ranges; invalid input/resource failures can still throw. |
| `workspace.close()` | `void` | Release the runtime and invalidate its plans. Idempotent after success; saved encrypted host records are unaffected. |
| `workspaceByteRangeFromUtf16(text, start: number, end: number)` | `TextRange & {unit: "utf8_bytes"}` | Convert valid zero-based, end-exclusive browser indexes through Core; reject invalid/split-character ranges rather than guessing. |

Every operation ID is a fresh host CSPRNG `Uint8Array(16)`. A `WorkspaceHead` is
`{revision: string, commitId: readonly number[]}`: a canonical decimal uint64
string plus 16 bytes. Compare both fields. Do not coerce the revision to a
JavaScript number, use a revision alone for compare-and-swap, or reuse a plan
after reopening a runtime.

Workspace development bounds are 1 MiB of prepared source, 10,000 supplied
findings, 20,000 committed entries and 16 MiB bundles, restore input and output.
Batches share the source,
finding and output budgets; their JSON transport is also bounded. A runtime
retains at most 32 live plans. These are rejection limits, not capacity or
security guarantees. The optional model has its own smaller input/token bounds.

### Prepared change calls

The same opaque `PreparedWorkspaceChange` interface is returned for single,
batch and recovery preparations. No method below authorizes copying text to
ChatGPT. A successful save/activation must still precede handoff.

| Call | Returns | Lifecycle role |
| --- | --- | --- |
| `change.preview()` | `WorkspacePreview` | Read a single-input preview and transformation ranges. Requires exactly one text input; a batch/recovery plan of another size rejects with `invalid_batch`. |
| `change.previews()` | `readonly WorkspacePreview[]` | Read previews in input order. Recovery has no text previews and returns an empty list. |
| `change.heads()` | `{expectedHead: WorkspaceHead, candidateHead: WorkspaceHead}` | Read the exact complete heads used by host save and activation. |
| `change.exportCandidateBundle()` | `Uint8Array` | Export sensitive plaintext only to the encrypting host; clear the temporary bytes. It does not commit or grant handoff permission. |
| `change.discard()` | `void` | Release a pending plan without changing committed mappings. Calling it on an already consumed/discarded/stale handle is an error, not an idempotent close. |

Preview/head/export reads leave a valid plan pending. Activation, discard, runtime
close or reclamation of a stale-parent plan ends access. No-op activation still
consumes its own plan; other valid current-head plans survive. A failed
activation preserves plans. Hosts must explicitly discard unused live plans.

### Optional model calls

Import `ModelSession` from `@datafog/wasm/models`. Construct a session through
`open`; it is independent of a workspace, credentials and durable mappings.

| Call | Returns | Lifecycle role |
| --- | --- | --- |
| `ModelSession.open(options: ModelSessionOptions)` | `Promise<ModelSession>` | Validate configuration and load pinned local model/tokenizer/runtime assets explicitly. No workspace is opened or saved. |
| `model.describe()` | `ModelDescriptor` | Copy configured backend/model/adapter/label/score-scale metadata. Available after close; not a readiness check. |
| `model.scan(text: string, config?: ScanConfig)` | `Promise<ModelScanResult>` | Route rule-supported selected types through Core rules and remaining configured types through Small. One model inference at a time. |
| `model.scanModel(text: string)` | `Promise<ModelScanResult>` | Model-only output for the configured prompt labels, including types ordinarily handled by rules. |
| `model.close()` | `Promise<void>` | Release runtime resources. Idempotent after success; rejects during active inference. Retry failed cleanup explicitly. Reopen with a new `ModelSession.open`. |

`ModelSessionOptions` requires `modelBaseUrl: string | URL`. Optional fields are
`wasmPaths: string`, `numThreads: number` (1–4, default 4 with automatic use of
one thread without cross-origin isolation), `labels: Record<string, string>`
(prompt → canonical entity type) and `threshold: number` (greater than 0 and at
most 1, default 0.5). Explicitly request one thread for the reference MV3 setup.
Schemas allow 1–16 unique, trimmed prompts of at most 64 characters without
brackets/control characters; Core also validates their canonical type mapping.

`ModelScanResult` contains `contractVersion: 1`, the model identity, canonical
`findings` and per-finding `evidence`. `SourceEvidence` carries detector identity,
optional native labels/raw scores and optional calibrated confidence. A raw pair
sigmoid score is not an accuracy or calibrated-confidence promise. These fields
do not implement the later workspace scanner/recall/user-source review feature.
Use `Finding.byteRange` when preparing a workspace, as in the example above.

## Errors and retry

For direct SDK calls, `DataFogError` exposes `code` and optional `reason`, `path`,
`findingIndex`, `inputIndex` and `markerIssues`. Branch on codes and handle
locations when present; do not parse messages or assume validation-error
precedence. Indexes are zero-based, with finding indexes local to the batch
input. Marker issues carry only kind and byte/codepoint/UTF-16 ranges. Render
them against the host's own local text, never as HTML or partially restored text.

### Workspace, request and capability codes

| Code | Meaning and host action |
| --- | --- |
| `invalid_configuration` | Unsupported/malformed configuration. Correct it before retrying. |
| `invalid_batch` | Invalid batch structure or a single-preview request on a plan with another number of inputs. Correct membership/use `previews()` and obtain fresh approval. |
| `invalid_finding` | Invalid text, type, supplied value/range, selection or overlap with an existing marker. Fix the local finding/selection and prepare again. |
| `invalid_operation_id` | Invalid public-ID bytes, including factory/operation IDs or head IDs. Use correctly sized host-random factory/operation IDs and unmodified heads from `change.heads()`; never replace a saved/candidate head ID with a new random value. |
| `invalid_marker` | Source preflight or strict restore blocked a marker. Highlight safe issue ranges and require edit/retry; no passthrough or partial restore. |
| `invalid_snapshot` | Bundle is malformed, incompatible or fails Core validation. Reject it; do not repair opaque bytes or overwrite saved state automatically. |
| `closed_workspace` | This runtime has closed. Rehydrate from validated durable state through the host unlock flow. |
| `inactive_plan` | Plan was consumed, discarded or reclaimed as stale. Prepare a new change and require new approval. |
| `workspace_conflict` | Wrong runtime/plan or complete-head mismatch. Reconcile from durable state and obtain fresh review; never force activation. |
| `resource_limit` | Workspace, transport, entry, plan or output budget exceeded. Discard unused live plans or reduce the operation; no silent truncation. |
| `unsupported_strategy` | Provider-backed stateless operation is unavailable in WASM. Use the supported stateless strategies or the separate Workspace flow. |
| `internal_error` | Unclassified SDK/transport failure. Release no new text and surface a safe failure; do not downgrade it to detection success. |

### Model codes

| Code | Meaning and host action |
| --- | --- |
| `model_load_failed` | Assets/runtime/setup failed to load or validate. Correct explicit setup and open a new session; no automatic rule-only fallback. |
| `model_inference_failed` | Inference/response validation failed. Release no new result; require an explicit retry or session recovery. |
| `model_resource_exhausted` | Model input/runtime budget exhausted. Reduce/split input explicitly while preserving offsets; do not truncate it silently. |
| `model_session_busy` | Inference is active. Wait for completion; invalidate stale approval/results after an edit or lock, then retry close or another request explicitly. |
| `model_session_closed` | Session has closed. Open a new session explicitly. |
| `model_cleanup_failed` | Runtime cleanup failed. Retain ownership and retry `close()` or destroy the owning context; do not resume inference on that closed session. |

Model pre-validation can also throw native JavaScript `TypeError` (for example
missing `modelBaseUrl` or malformed source strings) and `RangeError` (invalid
threshold before loading). Initialization/platform failures can be native
exceptions too. Catch these at the host boundary, stop the operation and show
safe guidance; do not assume every thrown value has a `code`, log raw runtime
messages with source text, or convert an exception to an empty successful scan.

### Shared provider codes

The complete declared `DataFogErrorCode` union also includes the following codes
for shared provider contracts. Browser WASM has no key/token-provider support;
these codes do not add it, and are not workspace credential-unlock errors.

| Family | Codes |
| --- | --- |
| Key provider | `key_provider_required`, `key_not_found`, `key_access_denied`, `key_provider_unavailable`, `invalid_key_material`, `key_provider_error` |
| Token provider/envelope | `token_provider_required`, `invalid_token`, `unsupported_token_version`, `token_not_found`, `token_expired`, `token_access_denied`, `invalid_token_material`, `token_provider_unavailable`, `token_provider_error` |

### Host persistence and worker errors

The reference host's [`HostError`/RPC boundary](../examples/workspace-extension/src/rpc.js)
is separate from SDK `DataFogError`. Errors crossing a worker boundary need safe
code/location serialization; do not rely on an Error prototype surviving RPC.
The reference host also forwards known workspace codes above. Its additional
codes are not SDK exports or promises of a built-in vault API:

| Host codes | Required response |
| --- | --- |
| `locked`, `cancelled`, `stale_approval` | Stop handoff, clear temporary views and require unlock or fresh review as appropriate. |
| `storage_failure`, `save_uncertain` | Release no text. Only a definite aborted save permits explicit retry of the pending change; after an uncertain acknowledgment reopen durable state and require fresh review. |
| `already_exists`, `not_found` | Correct the selected/create/import destination; do not overwrite an occupied workspace. |
| `invalid_credentials`, `invalid_envelope`, `invalid_backup`, `recovery_unavailable` | Reject the credential/envelope/backup operation and preserve existing saved state. Do not weaken validation to continue. |

## Prerelease and pilot boundaries

Pin the exact 0.5.0-beta.1 package. Snapshot schema 1 and the workspace contract remain
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
