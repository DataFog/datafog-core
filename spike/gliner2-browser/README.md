# GLiNER2.5 Small browser scan demo

This demo exercises `@datafog/wasm/models`: one pinned English Small model and
one ONNX Runtime Web WASM path, with no Python worker. Rust Core routes rules
and model entities, validates spans, and returns canonical findings with score
evidence. The demo displays both a table and complete JSON; input text stays in
the browser.

Use Node.js 24 or newer and the repository's Rust/wasm-bindgen tooling:

```sh
npm ci --prefix bindings/wasm
npm run build --prefix bindings/wasm
cd spike/gliner2-browser
npm ci
npm run download-model
npx playwright install chromium
npm test
npm run serve
```

Open <http://127.0.0.1:8765>, load the model, and scan. The entity selection uses
canonical names such as `PERSON`, `ADDRESS`, `EMAIL`, and `PHONE`. Email and phone
come from Core rules; Small supplies person, address, and the other model types.
`npm test` writes actual detections to `results/entities.json`.

See [the browser model API](../../docs/browser-model.md) for setup, custom prompt
mappings, offsets, transformations, score evidence, limits and lifecycle. The
runtime implementation lives only in `bindings/wasm/models`; this demo contains
no second detector implementation. [Third-party notices](../../bindings/wasm/models/THIRD_PARTY_NOTICES.md)
retain the decoder attribution and model licenses.

Tests exercise real WASM inference, positive and negative text, custom types,
Unicode/redaction, rule-only routing during inference, malformed and oversized
requests, missing/corrupt assets, one-thread fallback, and cleanup. Unit tests
cover reference token IDs, schema marker routing and decoder boundary cases.
