# Core SDK model qualification

Core v1 qualifies its public browser model SDK independently of the separately
shipped browser extension. A synthetic Chromium Manifest V3 reference host uses
a freshly packed and installed `@datafog/wasm` tarball. This proves SDK behavior;
it does not claim the extension's user interface, permissions, accuracy targets,
or release acceptance are complete.

## Run

Run doctor first. Explicit setup installs pinned project dependencies and browser
tools; the qualifier does not silently install them. Model provisioning downloads
the three pinned Small files (296,863,936 bytes total) and checks every size and
SHA-256 digest. `DATAFOG_BROWSER_MODEL_DIR` is shared by provisioning and the host.

```sh
cargo dfv doctor
npm ci --prefix bindings/wasm
npm ci --prefix examples/workspace-extension
npx --prefix examples/workspace-extension playwright install chromium
export DATAFOG_BROWSER_MODEL_DIR="$PWD/target/core-v1-browser-model/assets"
node spike/gliner2-browser/scripts/download-model.mjs
node --test scripts/test-browser-detection-scoring.mjs scripts/test-core-sdk-qualification.mjs
cargo dfv check --core-sdk --json
```

Use a frozen working tree during the final run. The entrypoint hashes the actual
Rust, WASM, harness, fixture and script inputs before and after execution, including
uncommitted files, and fails if they change. Reports go to a new ignored
`target/core-sdk-qualification/<timestamp>/` directory. `source-inputs.json`,
`receipt.json`, `summary.json`, `observations.jsonl` and `core-sdk-gate.json` retain
source identities, fresh tarball and consumer-lock hashes, browser/runtime versions,
verified model asset identities, actual observations, scores and checks.

## What blocks Core SDK qualification

- Model/artifact provisioning, build, fresh installation, browser execution, or
  inference fails, including an explicit model load failure.
- Any of the existing 64 synthetic records is missing, duplicated, or has invalid
  byte, codepoint, or UTF-16 spans.
- Approval/save ordering, cancelled approval, aborted save, restore, read-only
  recall, or one of the six restart/password/recovery recall phases fails.
- The browser accesses an external HTTP service or raises a page error.
- The receipt's actual observations do not reproduce its recorded scores, the
  pinned model identity is wrong, or inputs change during execution.

## What the scores mean

The existing synthetic corpus and one-to-one scorer remain unchanged. Every
record is measured through the actual Small model. Exact scores include complete
ADDRESS spans; street-line overlap remains a separate diagnostic. Types lacking
complete gold annotations are retained in raw observations and excluded from
these scoped scores. There is no new accuracy threshold or replacement baseline.
The historical preparation and current receipts remain in `baselines/` unchanged.

Approved-value recall uses explicit fixture approvals. Its zero-miss requirement
proves saved values survive restarts and recovery; it is not detector recall and
does not improve the model's raw measured accuracy. This synthetic set is not a
held-out real-world accuracy study. Native GLiNER2, GPU faults, WebKit, mobile
browser behavior and the shipping extension need their own evidence; this run
does not qualify them.
