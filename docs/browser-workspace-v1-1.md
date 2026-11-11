# Browser extension Workspace with embedded detection

Core 1.1.0 combines bundled CNN inference with reversible Workspace mappings
and either FogVault or SQLite3 Multiple Ciphers persistence. Install the stable 1.1.0 SDK and Core-owned adapters together; independent
FogVault remains 0.1.0. These additions are not in the 1.0.0 packages.

The detector runs in its own Worker. The selected vault adapter exclusively owns
the mutable Workspace and durable storage. A coordinator fences reviews when the
detector setting changes, and releases an approved result only after the adapter
confirms persistence and activates the saved candidate.

```js
import {init, Workspace} from '@datafog/wasm';
import {BrowserWorkspace} from '@datafog/wasm/cnn/workspace';
import {SQLiteVault, SQLiteWorkspace} from '@datafog/core-sqlite-browser';

await init();
const vault = await SQLiteVault.open({id: savedVaultId, password});
const workspace = await SQLiteWorkspace.open({
  Workspace, vault, recordId: savedRecordId, expectedNamespace: savedNamespace,
});
const owner = await BrowserWorkspace.open({workspace, cnnEnabled: savedPreference ?? true});
// Transfer exclusive use: subsequent mutations go through owner.
const review = await owner.prepare(text, crypto.getRandomValues(new Uint8Array(16)), {
  scan: {entities: ['PERSON', 'ADDRESS', 'EMAIL']},
});
showLocalPreview(review.preview());
// After explicit approval of this exact review:
const saved = await owner.commit(review, {approved: true});
showApprovedText(saved.previews[0].text);

// The extension UI persists the preference and applies it through the coordinator.
await owner.setCnnEnabled(false);
const original = owner.restore(saved.previews[0].text);
await owner.lock();
```

The application supplies the saved routing identifiers, password UI, local
preview and actual approval event. For new vault creation, backup, recovery and
credential changes, see the [SQLite adapter](../integrations/sqlite-browser/README.md).
The same coordinator accepts a [FogVault Workspace owner](integrations/fogvault.md).
Both adapters keep sending separate from saving; preserve the existing
conversation/send-observation contract for network handoff.

For conversation-bound protection, use the coordinator's
`prepareConversationSend([{input: {text, userSelections: []}, conversation}], operation, {scan})`.
It scans every input and captures the conversation route before awaiting inference.
Read `review.sendIntents()` before committing the review. A saved intent remains
pending through restart. Only a trusted actual-send observation, prepared with
`prepareObservedSend` and explicitly approved/saved through `commit`, authorizes
`restoreInConversation`. `prepareDefinitelyNotSent` records trusted non-delivery;
a timeout alone is insufficient. The coordinator never sends or invents these
observations.

Changing the setting invalidates pending reviews before awaiting Worker changes.
A change during a submitted save closes the owner and reports `reopen_required`;
reconcile/reopen durable state, then prepare and approve fresh work. Never reuse
the previous approval. Disabling detection preserves previously saved mappings.
Lock/close revokes authority immediately; caller-owned plaintext remains the
application's responsibility.

The [CNN guide](browser-cnn.md) describes default loading, supported statement
inputs, limits and packaged Worker assets. Keep the SDK, CNN Worker, Core WASM,
SQLite Worker and pinned SQLite WASM assets inside the extension. Run from a
trusted extension page/offscreen host whose lifetime supports these Workers.

## Candidate qualification

After doctor-first environment setup and all SDK builds:

```sh
cargo dfv build --browser-v1-1
cargo dfv check --browser-v1-1 --json
```

The check runs installed offline MV3 CNN tests, SQLite persistence/failure tests,
and the combined approved save/restoration flow with both backends. It includes
full browser-process restart with detection disabled and approved recovery from
FogVault into a fresh SQLite destination. Reports identify the
candidate archives and retain failure evidence. The SQLite profile does not
provide independent rollback detection for replayed valid storage. Automated
Chromium evidence does not replace acceptance in the actual packaged extension
or establish support for untested browser engines and devices.

Publication requires a successful `Browser CNN and Workspace vaults` workflow
for the exact release commit, alongside the existing release qualifications.
PR merge-checkout results alone do not authorize publication.
