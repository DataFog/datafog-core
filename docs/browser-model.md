# GLiNER2.5 Small browser scans

The optional `@datafog/wasm/models` entry point runs the pinned English
GLiNER2.5 Small ONNX model locally. It requires no Python worker. Importing the
SDK, including this entry point, does not fetch model assets or load ONNX Runtime.
The existing synchronous `scan()` and `scanAndTransform()` remain rule-based.

## Setup

Install `@datafog/wasm` with your JavaScript package manager. Host the three
model files from
[`nicolasembleton/gliner2.5-small-v1-onnx`, revision `5e2e3f51adfb0eeb7c1f83464400b4d498d41659`](https://huggingface.co/nicolasembleton/gliner2.5-small-v1-onnx/tree/5e2e3f51adfb0eeb7c1f83464400b4d498d41659)
at one base URL: `onnx/model.onnx`, `tokenizer.json`, and `tokenizer_config.json`.
Keep their upstream licenses. The graph and tokenizers total about 297 MB;
opening the session verifies every file's pinned size and SHA-256 digest.
Model files are downloaded separately from the npm package.

For repository development, `npm run download-model --prefix spike/gliner2-browser`
downloads and verifies those assets into the demo's ignored `models/` directory.
The demo supplies an import map for unbundled browsers. Bundlers resolve the npm
imports normally. Host ONNX Runtime Web 1.31.1's `dist` assets together; set
`wasmPaths` to that directory when your bundler does not serve them automatically.

```js
import {transform} from "@datafog/wasm";
import {ModelSession} from "@datafog/wasm/models";

const text = "Contact John Smith at john@example.com.";
const session = await ModelSession.open({
  modelBaseUrl: new URL("/models/gliner25-small/", location.href),
  wasmPaths: new URL("/onnxruntime/", location.href).href,
});
try {
  const result = await session.scan(text, {entities: ["PERSON", "EMAIL"]});
  console.log(result.findings, result.evidence);
  const redacted = transform(text, result.findings, {default: {strategy: "redact"}});
  console.log(redacted.text);
} finally {
  await session.close();
}
```

`open()` initializes DataFog's Rust WASM automatically. It loads one ONNX graph
with the WASM execution provider. It requests up to four threads when the page
is cross-origin isolated, otherwise one. Serve COOP `same-origin` and COEP
`require-corp` for threading, and allow the model/runtime assets under your CSP
and CORS policy. ONNX Runtime initializes per browser context; select its asset
path and thread settings consistently across sessions.

## Entities and results

The default prompt mapping is:

| Model prompt | DataFog entity type |
| --- | --- |
| person | PERSON |
| address | ADDRESS |
| email | EMAIL |
| phone number | PHONE |
| identification number | IDENTIFICATION_NUMBER |
| url | URL |
| username | USERNAME |

`ADDRESS` denotes the model's full address span and is distinct from
`STREET_ADDRESS`. The model is schema-driven, so callers can use other prompt
labels with an explicit canonical mapping when opening a session:

```js
const session = await ModelSession.open({
  modelBaseUrl: new URL("/models/gliner25-small/", location.href),
  labels: {person: "PERSON", company: "COMPANY", product: "PRODUCT"},
});
```

Mappings contain 1–16 unique prompts and unique canonical types. Prompts must be
trimmed, nonempty, at most 64 UTF-8 bytes, without brackets or control/format
characters. Canonical types start with an uppercase ASCII letter and contain
only uppercase ASCII letters, digits, and underscores, at most 64 bytes.

`session.scan(text, config?)` accepts the usual `ScanConfig` directly. Without a
configuration it runs all enabled rules and returns the configured model types
that rules do not cover. For explicit `entities`, Core rules own every type they
support, including `EMAIL` and `PHONE`; the model supplies only the requested
remaining types. A rule-only selection runs without inference, even while that
session's model is busy. Unknown types, duplicates and empty selections fail.
`session.scanModel(text)` returns every configured model type without rules.

Results contain `contractVersion: 1`, pinned `model` identity, `findings`, and
`evidence`. Findings use the same shape as the synchronous scanner and can be
passed to `transform()`. All ranges are zero-based, end-exclusive; UTF-8 byte,
Unicode code-point and UTF-16 code-unit positions select the exact source text.
Rust Core validates all model spans before filtering and combining them with
rules. Invalid model output or inference failures return no partial findings.

Evidence uses the shared Core field names: `finding_index` indexes `findings`,
and each `sources` entry contains `detector_name`, `detector_version`,
`native_label`, `raw_score`, and `calibrated_confidence`. Absent optional values
are `undefined` in JavaScript and omitted by `JSON.stringify`. Small's sigmoid
scores use `raw_score: {value, scale_id: "gliner25_pair_sigmoid"}`. They are
uncalibrated; Small findings leave `confidence` absent. `threshold` at open time
sets the raw score cutoff (default 0.5; greater than zero, at most one).

## Bounds and lifecycle

Inputs are limited to 65,536 UTF-8 bytes. Model requests also reject inputs
exceeding 512 tokens, including the prompt schema. Text is never silently
truncated; split larger documents into passages. Core output is limited to 1,024
findings and 4 MiB of serialized findings/evidence before adding UTF-16 ranges. JavaScript rejects malformed
Unicode before crossing the WASM boundary.

A session admits one inference at a time. Another model request, or `close()`
during inference, rejects with `model_session_busy`. Reuse sessions across calls;
await all scans before closing. Successful close is idempotent, concurrent close
calls share cleanup, and later scans fail with `model_session_closed`. A failed
release reports `model_cleanup_failed`; another `close()` can retry cleanup.

This first browser integration has no inference cancellation, inference deadline,
Web Worker isolation, or automatic document chunking. Asset fetches time out
after 120 seconds. Inference shares the browser's JavaScript context and may
pause the UI. `DataFogError` reports sanitized model load, inference, resource,
and cleanup failures; no input text or runtime details are included.

English is the supported language. Prompt acceptance does not establish accuracy
for a type: the checkpoint can miss entities, return partial addresses, and
produce false positives. For example, it can label “quarterly report” as a
person. This integration does not claim production PII qualification.
