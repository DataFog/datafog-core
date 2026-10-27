# GLiNER2.5 Small browser extraction

One pinned GLiNER2.5 Small model, one ONNX Runtime Web WASM path, and one
entity-extraction API. The browser needs no Python worker or PyTorch runtime.
This is an isolated browser spike; it does not change the existing SDK workers
or the persistent-workspace stack.

## Run

Use Node.js 24 or newer for the tooling:

```sh
cd spike/gliner2-browser
npm ci
npm run download-model
npx playwright install chromium
npm test
npm run serve
```

Open <http://127.0.0.1:8765>, load the model, and extract entities. The UI
returns both a table and the complete JSON result. It never sends the input
text to a model server. `npm test` writes actual demo detections to
`results/entities.json`.

## Entity types

The model is schema-driven: it extracts the types requested for each call.
There is no fixed exhaustive list to enumerate, and it is not limited to
names and addresses. The PII-oriented default is:

```js
[
  "person",
  "address",
  "email",
  "phone number",
  "identification number",
  "url",
  "username",
];
```

Callers can supply other labels without exporting or downloading another model:

```js
import { GlinerSession } from "/src/session.mjs";

const session = await GlinerSession.create();
try {
  const result = await session.extract(
    "Apple CEO Tim Cook announced iPhone 15 in Cupertino yesterday.",
    { labels: ["company", "person", "product", "location"], threshold: 0.5 },
  );
  console.log(result.entities);
} finally {
  await session.close();
}
```

Each entity contains `label`, exact source `text`, `score`, `byteRange`,
`codepointRange`, and `utf16Range`. Every range is zero-based and end-exclusive.
`text.slice(entity.utf16Range.start, entity.utf16Range.end)` selects the entity.
Labels are returned unchanged, without mapping arbitrary extraction types to
DataFog's canonical PII enum. These are experimental model extraction results,
not automatically integrated DataFog findings or transforms.

## Model and runtime

- Model: [`nicolasembleton/gliner2.5-small-v1-onnx`](https://huggingface.co/nicolasembleton/gliner2.5-small-v1-onnx/tree/5e2e3f51adfb0eeb7c1f83464400b4d498d41659), revision `5e2e3f51adfb0eeb7c1f83464400b4d498d41659`.
- The English 74M-parameter checkpoint's main graph supports entity extraction;
  no optional relation, attribute, or record heads are loaded.
- Download: about 297 decimal MB for the graph and tokenizer assets.
- `src/model.mjs` pins expected sizes and SHA-256 hashes. Both the downloader
  and browser verify assets; corrupt, incomplete, or missing files fail explicitly.
- `@huggingface/tokenizers` handles tokenization directly. No Transformers.js
  pipeline, native ONNX runtime, handwritten subword tokenizer, or remote inference is used.
- WASM uses up to four threads with the server's cross-origin isolation headers,
  and falls back to one thread when isolation is unavailable.
- See [third-party notices](THIRD_PARTY_NOTICES.md) for decoder attribution and licenses.

## Limits and checks

The spike rejects input exceeding 512 model tokens including its label schema,
or 65,536 UTF-16 code units, rather than silently truncating it. Split larger
input into passages. Calls accept 1–16 distinct labels of at most 64 characters,
and a threshold greater than 0 and at most 1. Concurrent calls on one session
are rejected; close the session when finished.

Labels condition the model's predictions; accepting a label does not establish
reliable accuracy for that entity type. The checkpoint can miss entities,
return partial addresses, or produce false positives. For example, it can
label “quarterly report” as a person in “The quarterly report is ready for review.”
This change selects a browser default; it does not assert production qualification
or that the previous PERSON recall requirement has been met.

Tests exercise actual WASM detections, a genuine negative case, configurable
non-PII labels, empty and invalid input, Unicode ranges, missing/corrupt assets,
single-thread fallback, and session cleanup. Unit checks cover token IDs from
an independent reference, padded/duplicate candidate decoding, thresholding,
and rejection of oversized input. There are no native parity paths or
cross-model comparison commands.
