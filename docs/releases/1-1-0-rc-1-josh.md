# DataFog 1.1.0-rc.1 extension test instructions

Josh — this is the installable RC for the extension. The CNN is bundled into Core
WASM and loads by default through `BrowserWorkspace.open`. SQLite3 Multiple
Ciphers is available alongside FogVault. Please test your packaged extension
before we promote the RC to stable.

## Install the exact candidate

In your extension project:

```sh
npm install --save-exact @datafog/wasm@1.1.0-rc.1 @datafog/core-sqlite-browser@1.1.0-rc.1 @datafog/core-fogvault-browser@1.1.0-rc.1 @datafog/fogvault-browser@0.1.0
npm ls @datafog/wasm @datafog/core-sqlite-browser @datafog/core-fogvault-browser @datafog/fogvault-browser
```

Use these exact versions rather than `@next`, which can move to a later RC.
FogVault's independent engine stays at 0.1.0. The CNN does not need a Python
process, an ONNX session, or a separate weight download.

## Keep the runtime assets in the extension

Build your extension normally. Download
[copy-browser-rc-assets.mjs](https://github.com/DataFog/datafog-core/releases/download/v1.1.0-rc.1/copy-browser-rc-assets.mjs)
and, from your extension project directory, pass your actual unpacked build
directory to it. For example, if your build output is `dist`:

```sh
node /path/to/copy-browser-rc-assets.mjs ./dist
```

The helper checks the installed versions and copies each package under
`dist/datafog/`, preserving Worker/WASM relative paths. It writes
`dist/datafog/assets.json` with versions and hashes. Run it after rebuilding if
your build cleans its output directory.

Run Core, vault ownership and inference in a trusted extension-owned page or
offscreen host with the required lifetime. Keep keys and Workspace handles out
of content scripts. Add `'wasm-unsafe-eval'` to the extension-page `script-src`
and allow `worker-src 'self'` in the MV3 CSP. Preserve your existing network
policy for actual sends. These local assets do not require public
`web_accessible_resources` exposure.

With the copied layout, imports from an extension page can use:

```js
const {init, Workspace} = await import(chrome.runtime.getURL('datafog/wasm/index.js'));
const {BrowserWorkspace} = await import(chrome.runtime.getURL('datafog/wasm/cnn/workspace.mjs'));
const {SQLiteVault, SQLiteWorkspace} = await import(chrome.runtime.getURL('datafog/core-sqlite-browser/index.mjs'));
await init();
```

Open/create the selected vault and its Workspace owner, then transfer ownership:

```js
const owner = await BrowserWorkspace.open({
  workspace: vaultWorkspace,
  cnnEnabled: savedPreference ?? true,
});
console.log(owner.cnnEnabled, owner.detectorStatus); // true, ready by default
// UI toggle: persist the preference, then apply it through the coordinator.
await owner.setCnnEnabled(false);
```

`vaultWorkspace` is either a `SQLiteWorkspace` or `FogVaultWorkspace` from the
selected adapter. Persist the randomly generated vault/record/namespace routing
IDs once and reuse them on reopen. Use the owner exclusively after transfer.
Existing synchronous `scan()` stays rule-based; importing the SDK alone does not
enable CNN inference. The extension must adopt this new flow explicitly.

## Acceptance run

Use a fresh test vault with synthetic data. Run this once with SQLite and once
with FogVault in the actual packaged extension and intended retail browser.

Start with this text, selecting `PERSON`, `ADDRESS` and `EMAIL`:

```text
BILL TO
John Smith
123 Main Street
Springfield IL 62704
USA
Email john@example.test
```

1. **Default startup:** confirm `cnnEnabled === true` and `detectorStatus === 'ready'`.
   Check that name/address findings reach your actual review UI, alongside the
   email rule finding. Test representative extracted text from your own input
   path too; preserve its line breaks.
2. **Review and approval:** preparing/showing a preview must not authorize a send.
   Only an explicit approval followed by confirmed save/activation may return the
   text for handoff. Cancelled/rejected approval must leave saved state unchanged.
3. **Toggle:** turn CNN off and scan the sample again before committing its
   mappings. PERSON/ADDRESS inference should disappear while EMAIL still works.
   Re-enable it and confirm the findings return. Changing the setting while a
   review is pending must reject that old review/approval. Persist the preference
   through an extension/browser restart.
4. **Save/reopen/restore:** approve a protected result, retain its markers, lock,
   reopen and restore the exact original. Repeat after closing the entire browser.
   Restoration must still work with CNN disabled. Disabling detection must not
   delete previously committed mappings.
5. **Failure/lifetime:** reject a wrong password. Close/lock the owner during
   inference and near a save. An error, late reply or uncertain save must not
   authorize handoff. Reopen/reconcile and require a fresh review/approval.
6. **Conversation flow:** use `prepareConversationSend`, capture `sendIntents()`
   before committing, and confirm a saved send remains pending across restart.
   Only your trusted actual-send observation, prepared with `prepareObservedSend`
   and explicitly approved/saved, may enable scoped restoration. No automatic
   resend, fabricated observation, or cross-conversation restore.
7. **Packaging/performance:** confirm no missing Worker/WASM files or CSP errors,
   no external model/runtime requests, and a responsive UI on normal hardware.
   Check service-worker suspension and the lifetime of your actual page/offscreen
   owner. Try realistically sized inputs and record misses, false positives,
   delays and memory concerns.

If changing CNN settings during a submitted save reports `reopen_required`, that
is the intended conservative result: reopen durable state and prepare/approve
fresh work. A missing reply does not prove the save rolled back.

Please send back: extension commit/build ID, browser/OS, exact package versions,
`datafog/assets.json`, backend tested, pass/fail for each item, and synthetic repro
text plus the error code for any failure. Do not include real customer PII or
vault credentials.

The model targets statement recipient names and mailing-address blocks; testing
the extension does not establish universal name/address accuracy. A Workspace
that uses mapping retirement writes snapshot v4, which Core 1.0 cannot reopen.
The SQLite browser profile cannot independently detect replay of an older valid
profile. See the [integration guide](../browser-workspace-v1-1.md) for the owner
and recovery contracts.
