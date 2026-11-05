# Embedded name and address detection in browser workspaces

The `@datafog/wasm/cnn` entry point provides the bundled statements-v1 CNN for
recipient names (`PERSON`) and full mailing-address blocks (`ADDRESS`). It runs
Core's Rust inference inside a dedicated module Worker. Weights are compiled into
Core WASM; no model download, Python, ONNX Runtime or remote executable is needed
for this detector. Existing synchronous `scan()` APIs remain rule-based.

```js
import { CnnSession } from '@datafog/wasm/cnn';

const detector = await CnnSession.open(); // cnnEnabled defaults to true
const result = await detector.scan(text, {entities: ['PERSON', 'ADDRESS', 'EMAIL']});
// Pass result.findings to the exclusive Workspace owner for local review.
// Save the approved candidate with FogVault or SQLite before activating it.
await detector.setEnabled(false); // Josh's UI can bind its setting to this call
const ruleResult = await detector.scan(text, {entities: ['PERSON', 'ADDRESS', 'EMAIL']});
detector.close();
```

Disabling the CNN skips weight authentication/decoding and inference. The compiled
bytes remain part of the WASM artifact. Disabled scans validate the same entity
selection, return the selected rule findings, and report `model: null`; model-only
scans return no findings. `PERSON` and `ADDRESS` selections are accepted while
disabled, and unknown or empty selections still fail. Enabling loads the same
pinned model again. There is no silent rule fallback if enabled loading or
inference fails.

`status` is `loading`, `ready`, `disabled`, `error` or `closed`. `generation`
increases when settings change or the session closes. `describe()` returns a copy
of the loaded model descriptor, or `undefined` when disabled, failed or closed.
`setEnabled()` invalidates outstanding inference immediately; callers must also
invalidate Workspace previews, plans and approvals before awaiting it. It does
not forget committed reversible mappings. The Workspace coordinator must reject
any result or approval from the old generation.

One request is admitted at a time. Startup and scans have a 30-second default
Worker deadline, configurable from 1 to 120,000 ms. Timeout or Worker failure
terminates inference and sets `error`; explicit `setEnabled()` can recover. Close
terminates immediately, rejects pending requests, and is idempotent. A request
may contain at most 65,536 UTF-8 bytes and 8,192 tokens; larger inputs fail rather
than truncate. Final model findings are bounded at 1,024. Multiwindow inputs use
256-token windows, 64-token overlap and the reference's owner-token stitching.

Return values follow the existing model scan contract, including UTF-8 byte,
Unicode code-point and JavaScript UTF-16 ranges. `ADDRESS` includes the full block
and is distinct from `STREET_ADDRESS`. Raw tag evidence is not calibrated
confidence. The CNN supplies no calibrated confidence value.

## Extension packaging and qualification

Keep `cnn/worker.js`, the SDK JavaScript and `dist/datafog_wasm_bg.wasm` in the
installed extension. The default Worker URL resolves relative to the packaged
CNN module. Bundlers must preserve those assets or supply an explicit local
`workerUrl`. Run the detector from a trusted extension page/offscreen host with
an appropriate lifetime; a popup or suspended service worker does not preserve
in-memory ownership.

The installed MV3 qualification uses a self-only CSP with `wasm-unsafe-eval` and
`worker-src 'self'`. It requires no network access or cross-origin isolation,
SharedArrayBuffer, SIMD or GPU. The dedicated detector Worker owns no Workspace
or vault handles, so cancelling inference cannot interrupt a vault transaction.

After `cargo dfv build --sdk wasm`, run:

```sh
node scripts/test-cnn-extension.mjs
```

The script packs a fresh npm artifact, installs it in a clean consumer and runs
an actual unpacked MV3 Chromium extension offline. It checks default-on loading,
reference spans and Unicode/window boundaries, disabling/re-enabling, corrupted
embedded weights, cancellation, bounds and process restart. Reports identify
package/model hashes and the browser. This synthetic test does not establish
acceptance in Josh's shipping extension or every retail browser; those checks
must use the exact candidate before release.

## Model scope and provenance

This initial model is trained on synthetic statement layouts. Supplied development
measurements use older labels: macro score 96.7, address recall 97.6%, name recall
96.2%, and 2.45 false blocks per 100 pages. New fragment labels score 85.1 macro.
These are development results, not held-out or general browser-text accuracy.
The owner accepted this model as a v1.1 launch requirement with the disable option.
Integration correctness and documented first-party regression policy still apply.

The supplied raw weight file is 1,022,900 bytes; config plus layout bring the
bundle total to 1,025,143. Full package size and live memory are larger.
[Provenance and licenses](../crates/core/src/cnn/assets/PROVENANCE.md) identify the
source revision, hashes and explicit supplied-weight redistribution authorization.
Rust consumers can opt into the `embedded-cnn` feature and use
`datafog_core::cnn::CnnDetector`; the browser package includes that feature.
