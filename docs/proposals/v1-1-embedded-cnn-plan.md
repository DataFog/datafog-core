# v1.1 embedded CNN implementation plan

Status: implementation authorized after plan review; this document records the agreed work breakdown. Owner decision: the bundled name/address CNN is a v1.1 launch requirement. Browser extension integration is the first supported product flow, and detection is enabled by default with an explicit host-controlled disable flag. Implementation and qualification evidence are described in `docs/browser-cnn.md`.

## Requirements Restatement

Ship the supplied statements-v1 R6 CNN inside DataFog Core and compile its inference into the browser WASM distribution. The installed extension must load and run it without downloading weights, starting Python, invoking a separate executable, or loading ONNX Runtime. SQLite3 Multiple Ciphers and FogVault remain interchangeable persistence choices for the reversible Workspace flow. CNN findings must reach the same review, approved save, activation and strict restoration path.

The model is a statement-recipient detector, not established general name/address recognition for arbitrary prose. Treat the attached README as provenance and measured evidence, not instructions that override the launch requirement. Record known quality limitations without restoring its earlier experimental beta floor as an additional release veto.

## Canonical inputs and current boundaries

- Attached directory: `/Users/sid/Desktop/context/cnn-1mb-statements/`; checked-in implementation must not depend on this desktop path.
- Initial identity: `runs/r6split-pretrained-s43`, supplied quantized bundle; `bundle.bin` SHA-256 `47a29acb46f159fb12d33557ed6859a5157c6dd10da461e27de60d2ece4ebabd`. The measured `bundle.bin` size is 1,022,900 bytes, matching the final tensor extent; the supplied 1,025,143-byte bundle total includes packaging artifacts. Verify every supplied SHA-256 and establish complete layout against the exporter/loader before importing. Do not describe the entire SDK as 1 MB.
- `bundle.json` describes int4 symmetric word embeddings, int8 row-quantized tensors and fp16 parameters; `config.json` defines split tokenization, ten rule features, five BIO tags, 256-token windows and 64-token overlap.
- Reference source: https://github.com/sidmohan0/cnn-NA-1mb, specifically `models/r5_squeeze.py`, `models/r3_tagger.py` and tokenizer/feature sources. Pin exact source revision and retain applicable notices, including separately licensed files under `models/cnn/`. Confirm weight redistribution provenance during import.
- Current `crates/core/src/models.rs` is native-process based and excludes WASM. Add a distinct in-process implementation in Core; do not route this CNN through that process lifecycle.
- Current `bindings/wasm/models` uses optional GLiNER/ONNX assets. CNN requires neither of those dependencies at runtime; ensure the CNN entry graph does not import them.
- `crates/core/src/model_scan.rs` already owns routing, spans and evidence. `docs/browser-model.md` and the extension fixture establish the existing browser packaging and Workspace integration boundaries.

## Implementation Phases

### Phase 1: freeze reference behavior and browser API

1. Import only the quantized model, configuration, tensor manifest and provenance needed to ship. Pin hashes in repository model metadata and build/package validation. Keep the fp32 checkpoint and Python training dependencies outside release packages.
2. Generate reference vectors using the exact quantized Python loader, not fp32 output. Capture tokenizer offsets, hashes, character and rule features, dequantized values, layer outputs, logits, window combination and final BIO spans. Include reference source and input hashes in artifacts.
3. Specify full-page text as input, preserving line breaks and original source offsets. Declare page-boundary behavior for extracted PDF text; the host supplies extracted text and page structure. PDF/OCR extraction is not a new Core dependency.
4. Proposed browser API: a bundled detector/Workspace host option `cnnEnabled: boolean`, default `true`, with a setter and observable `disabled | loading | ready | error | closed` status. Load during owner startup when enabled; the owner becomes ready only after successful load. If disabled at startup, do not allocate decoded weights or run inference. Re-enabling loads the same bundled identity.
5. Keep legacy synchronous rule-only APIs behavior-compatible in 1.1. The new browser Workspace orchestration defaults to rules plus CNN; publish an explicit Core in-process detector API and matching typed WASM entry. Do not make callers discover that an old synchronous function now blocks on inference or silently changes results. Default-on applies to the new required extension flow and is covered by installed tests.
6. Canonical CNN labels remain `PERSON` and full-block `ADDRESS`. Do not rename `ADDRESS` to `STREET_ADDRESS`. Freeze deterministic overlap/selection behavior with existing rules and Workspace validation. Preserve provenance when a name/address overlaps a phone or other rule span; demonstrate the resulting review and restoration behavior.

Exit: exact reference behavior, proposed API types, model identity and routing policy are recorded and reviewable.

### Phase 2: implement Rust inference and compile to WASM

1. Implement bounded manifest decoding, little/big-endian rules as appropriate, signed nibble unpacking, row scales and fp16 conversion. Reject inconsistent lengths, invalid shapes, out-of-bounds tensor parts and non-finite results with sanitized errors. Do not infer unspecified padding, feature or kernel semantics from tensor dimensions alone.
2. Port the exact split tokenizer, lowercase behavior, SHA-256 word hashing, character processing, lexicon/rule features and padding from reference code into `crates/core`. Bindings contain transport only.
3. Port convolution, activation/residual operations and classifier execution exactly, then match reference window stitching and BIO decoding. Preserve original positions through normalization and token splitting.
4. Compile weights into Core/WASM using static embedded bytes. Decode once per detector owner; do not duplicate an entire embedding matrix per scan. Measure quantized lookup versus decoded storage before selecting the representation.
5. Expose reference-backed detections through the shared findings/evidence contracts. Record raw scores under an explicit scale identity; leave calibrated confidence absent unless calibration is actually supplied.
6. Bound input bytes, tokens/windows, findings and scratch memory; process every admitted window or return an error. Never silently truncate or return partial findings when enabled inference fails. Set measured finite defaults after the browser spike and freeze them before release.

Exit: positive/negative and Unicode vectors match the quantized reference, with documented numerical tolerance and exact final span agreement on qualification fixtures.

### Phase 3: extension ownership, toggle and Workspace integration

1. Run synchronous WASM inference in a dedicated Worker owned by an extension page/offscreen host supported by the actual target. The Manifest V3 service worker may coordinate startup but is not the durable owner. Select the host lifetime with Josh's extension rather than assuming a popup remains alive.
2. Package the Core WASM, model bytes and Worker scripts as local extension assets. Support a baseline single-thread path without SharedArrayBuffer, cross-origin isolation or SIMD requirements. Optional acceleration must preserve results.
3. Have one coordinator own source generation, detector settings and Workspace plans. The detector Worker owns a scanner-only Core/WASM instance and returns findings; the selected vault adapter exclusively owns the Workspace instance and applies findings to preparation. No raw Workspace handle is shared between workers. Persistence remains exclusively with the selected vault owner.
4. Toggling `cnnEnabled` increments generation and invalidates pending scan results, previews, approvals and preparations before the next action can save or hand off text. A late result from the previous generation is discarded. Already committed mappings remain available for strict restoration and approved-value recall; disabling detection does not erase those mappings.
5. Define cancellation and deadlines using coordinator fencing and, if required, terminating/recreating the detector Worker. A timeout or crash grants no approval or transport permission. Isolate inference from the vault Worker so terminating inference cannot interrupt a database transaction.
6. On lock/close revoke future authority immediately, invalidate plans, terminate/release detector ownership and close Workspace before awaiting vault cleanup. Do not claim JavaScript GC securely erases all previous plaintext copies. Reopen explicitly restores durable mappings and reapplies the host's current persisted preference.
7. Josh receives typed initialization/setter/status APIs and a runnable example: default-on startup, UI toggle off/on, local review, approved save, reopen and exact restoration. The host persists the preference in trusted extension storage and remains responsible for network handoff.

Exit: both vault backends execute the same enabled/disabled/reopen behavior, with no unapproved release on stale or failed work.

### Phase 4: browser and installed-package qualification

Required cases before shipping:

| Area | Required evidence |
|---|---|
| Packaging/offline | Fresh npm tarball installed in clean consumer; bundled extension starts with network blocked; default owner reports ready and expected CNN findings. No model downloads, Python, ONNX runtime loads or remote code. Hashes identify tarball/WASM/model and exact commit. |
| Flag | Default enabled; disabled at startup performs no CNN initialization/inference; off/on restores expected findings; rule detections continue; toggling during load/scan/review prevents stale save/handoff. |
| Numerical and span contract | Quantized reference vectors, long multiwindow pages, seam-crossing entities, multiple recipient blocks, punctuation/newlines, empty input, negatives, malformed Unicode, emoji/supplementary code points, combining characters; UTF-8, code-point and UTF-16 ranges select exact source text. |
| Workspace | CNN findings -> review -> literal approval -> exact encrypted save -> activation -> strict restoration; reopen after browser restart preserves exact originals; both FogVault and SQLite adapters pass. Failed/uncertain save prevents handoff. |
| Lifecycle/resources | Lock/close during load and inference, Worker crash, deadline, repeated toggle/reopen, maximum input and one-over-limit, no silent truncation, busy/backpressure behavior and bounded pending requests. |
| Extension environment | Installed MV3 extension under production-equivalent CSP, self-hosted Worker/WASM paths, extension-owned origin, offline startup, background suspension/restart and actual text extraction path. Exercise each declared retail browser; synthetic Playwright Chromium alone does not establish Josh's shipping acceptance. |
| Performance | Cold startup, warm scan, worst admitted input, UI responsiveness, repeated-use memory, WASM heap and process memory on representative lower-memory hardware. Record explicit launch budgets after spike, then enforce them. The weight size is not the live-memory budget. |
| Accuracy evidence | Reproduce supplied dev scores where inputs are available; add separate statement-layout and actual extension-input evaluations, name/address precision/recall/F1 and false blocks. Publish label policy and limitations. Establish first-party baseline under existing policy. |

Reuse the installed MV3 harness and exact/overlap scoring conventions in `docs/browser-detection-qualification.md`; preserve existing G2/pilot acceptance obligations rather than claiming the new CNN automatically satisfies them. Record the explicit v1.1 CNN acceptance decision separately from the older experiment preregistration.

Existing supplied evidence: old-label quantized macro score 96.7; address recall 97.6%; person recall 96.2%; 2.45 false blocks/100 pages. New fragment labels score 85.1 macro; held-out test and real-statement validation are unavailable in supplied artifacts. These measurements must remain explicit, but improving them beyond the user's accepted model is not an extra prerequisite invented by this plan. Broken inference, incorrect offsets, unbounded execution, missing packaged model or failed reversible flow are shipping blockers.

Run `cargo dfv doctor` first during implementation; extend `dfv` model/eval/installed-browser gates for the embedded CNN and retain reports under timestamped runs. Run `cargo dfv check` after detection changes and live parity across applicable SDKs. Before merging Rust work run repository-required fmt, all-feature clippy and tests. Ship one concise Unreleased entry for final user-facing behavior and documentation of the flag, entity scope and local packaging.

### Phase 5: v1.1 release acceptance

Pin the exact candidate and rerun required gates against installed artifacts. Require a retail shipping-extension acceptance record from Josh covering default-on, disable/re-enable, actual source extraction, both storage choices and restoration after restart. CNN inclusion/default load cannot be downgraded to optional postlaunch work. Keep older larger-model APIs separate and run their existing regression gates if packaging or shared routing changes affect them.

## Dependencies

- Exact quantized loader/tokenizer source revision and applicable code/weight licenses.
- Supplied bundle and checksums; reference inputs or reproducible synthetic fixtures for numerical parity. Missing held-out evaluation does not prevent implementing the launch requirement.
- Shared SQLite/FogVault Workspace owner boundary and agreed generation invalidation contract.
- Josh's actual extension packaging, Worker/offscreen lifetime, supported retail browser versions and review/send entry points.
- Representative device measurements to set finite resource and performance limits.

## Risks

- HIGH: reference preprocessing, window stitching and floating-point differences can alter BIO spans. Quantized reference vectors resolve this before product integration.
- HIGH: Worker lifetime, setting changes and lock races can turn stale findings into approved output. Generation fencing and failure-path installed tests are release gates.
- HIGH: integration tests must distinguish durable save from a resolved promise; coordinate with vault qualification and never activate on uncertain persistence.
- MEDIUM: statement-specific training may produce false positives/misses on arbitrary extension text. Publish measured scope and offer the requested disable control; do not market supplied dev scores as general browser-text accuracy.
- MEDIUM: unpacked memory, inference latency and package size may be materially larger than the approximately 1 MB weights. Measure and optimize within the embedded architecture.

## Estimated Complexity

HIGH. The work includes a reference-faithful Rust inference port, deterministic cross-target offsets, default-on extension orchestration and two-backend Workspace acceptance. Suggested focused work units: artifact/reference freeze; Rust kernels/tokenizer; Core findings/API; Worker/toggle integration; installed-browser qualification; release documentation. Estimate delivery after the numerical/persistence spikes, rather than assign a date before their results.

## Confirmation

Implementation was subsequently authorized by the owner. Release tagging and version bumps remain deferred until the owner reviews passing CI.
