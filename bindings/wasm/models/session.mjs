import * as ort from "onnxruntime-web/wasm";
import { Tokenizer } from "@huggingface/tokenizers";
import { MODEL, validateLabels } from "./model.mjs";
import { prepareInput } from "./preprocessor.mjs";
import { decodeEntities } from "./decode.mjs";

async function verifiedAsset(baseUrl, path) {
  const response = await fetch(new URL(path, baseUrl), {
    signal: AbortSignal.timeout(120000),
  });
  if (!response.ok)
    throw new Error(`Cannot load ${path}: HTTP ${response.status}`);
  const buffer = await response.arrayBuffer();
  const expected = MODEL.files[path];
  if (buffer.byteLength !== expected.bytes)
    throw new Error(`Model asset size mismatch: ${path}`);
  const digest = await crypto.subtle.digest("SHA-256", buffer);
  const hash = Array.from(new Uint8Array(digest), (value) =>
    value.toString(16).padStart(2, "0"),
  ).join("");
  if (hash !== expected.sha256)
    throw new Error(`Model asset checksum mismatch: ${path}`);
  return buffer;
}

export class GlinerSession {
  static async create({ modelBaseUrl, wasmPaths, numThreads = 4 } = {}) {
    if (!modelBaseUrl) throw new TypeError("modelBaseUrl is required");
    if (!Number.isInteger(numThreads) || numThreads < 1 || numThreads > 4)
      throw new RangeError("numThreads must be between 1 and 4");
    const base = new URL(
      String(modelBaseUrl).replace(/\/?$/, "/"),
      import.meta.url,
    );
    const [definition, config] = await Promise.all(
      ["tokenizer.json", "tokenizer_config.json"].map(async (path) =>
        JSON.parse(new TextDecoder().decode(await verifiedAsset(base, path))),
      ),
    );
    const tokenizer = new Tokenizer(definition, config);
    if (wasmPaths !== undefined) ort.env.wasm.wasmPaths = wasmPaths;
    ort.env.wasm.numThreads = globalThis.crossOriginIsolated ? numThreads : 1;
    const buffer = await verifiedAsset(base, "onnx/model.onnx");
    const session = await ort.InferenceSession.create(buffer, {
      executionProviders: ["wasm"],
      graphOptimizationLevel: "all",
    });
    return new GlinerSession(session, tokenizer);
  }

  constructor(session, tokenizer) {
    this.session = session;
    this.tokenizer = tokenizer;
    this.busy = false;
  }

  async extract(text, { labels, threshold = 0.5 } = {}) {
    if (!this.session) throw new Error("Session is closed");
    if (this.busy) throw new Error("An extraction is already running");
    if (typeof text !== "string") throw new TypeError("text must be a string");
    if (!text.isWellFormed())
      throw new TypeError("text must contain valid Unicode");
    if (text.length > 65536)
      throw new RangeError(
        "Text exceeds 65,536 UTF-16 code units; split it into smaller passages",
      );
    if (!Number.isFinite(threshold) || threshold <= 0 || threshold > 1)
      throw new RangeError("threshold must be greater than 0 and at most 1");
    validateLabels(labels);
    const activeLabels = [...labels];
    if (!text.trim()) return { entities: [], labels: activeLabels };
    this.busy = true;
    const feed = {};
    let outputs;
    try {
      const prepared = prepareInput(text, activeLabels, this.tokenizer);
      const int64 = (values, dims) =>
        new ort.Tensor("int64", BigInt64Array.from(values, BigInt), dims);
      const mask = (length) =>
        new ort.Tensor("float32", new Float32Array(length).fill(1), [
          1,
          length,
        ]);
      Object.assign(feed, {
        input_ids: int64(prepared.inputIds, [1, prepared.inputIds.length]),
        attention_mask: int64(
          prepared.inputIds.map(() => 1),
          [1, prepared.inputIds.length],
        ),
        text_word_indices: int64(prepared.wordIndices, [
          1,
          prepared.words.length,
        ]),
        text_word_mask: mask(prepared.words.length),
        query_marker_indices: int64(prepared.queryIndices, [
          1,
          activeLabels.length,
        ]),
        query_marker_mask: mask(activeLabels.length),
        cls_marker_indices: int64([0], [1, 1]),
        cls_marker_mask: new ort.Tensor("float32", new Float32Array(1), [1, 1]),
        rel_marker_indices: int64([0], [1, 1]),
        rel_marker_mask: new ort.Tensor("float32", new Float32Array(1), [1, 1]),
      });
      outputs = await this.session.run(feed, [
        "pair_indices",
        "pair_logits",
        "pair_valid",
      ]);
      return {
        entities: decodeEntities(
          outputs,
          prepared,
          text,
          activeLabels,
          threshold,
        ),
        labels: activeLabels,
      };
    } finally {
      try {
        for (const tensor of [
          ...Object.values(feed),
          ...Object.values(outputs || {}),
        ])
          tensor.dispose();
      } finally {
        this.busy = false;
      }
    }
  }

  async close() {
    if (this.busy) throw new Error("Cannot close a session during extraction");
    if (this.session) {
      await this.session.release();
      this.session = null;
    }
  }
}
