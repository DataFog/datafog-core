import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { AutoTokenizer, env } from "@huggingface/transformers";
import * as ort from "onnxruntime-node";

import { decodeEntitySpanScores } from "./decode.mjs";

const MODEL_ID = process.env.GLINER2_ONNX_REPO ?? "okasi/gliner2-privacy-filter-pii-multi-onnx";
const ONNX_MODEL_FILE = process.env.GLINER2_ONNX_FILE ?? "model_q8.onnx";
const TEXT =
  process.env.GLINER2_TEXT ??
  "John Doe lives at 742 Evergreen Terrace, email john@example.com.";
const THRESHOLD = Number(process.env.GLINER2_THRESHOLD ?? "0.5");

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../../..");
const localModelDir = resolve(repoRoot, "artifacts/onnx/fastino__gliner2-privacy-filter-PII-multi");

env.allowRemoteModels = true;

function resolvePythonHelper() {
  return {
    helper: resolve(here, "../preprocess_feed.py"),
    runner: resolve(repoRoot, "modeling/gliner_finetune/run_with_python.sh"),
  };
}

function buildFeedWithPython(modelPath) {
  const { helper, runner } = resolvePythonHelper();
  const raw = execFileSync(
    runner,
    [
      helper,
      "--model-dir",
      modelPath,
      "--onnx-model-file",
      ONNX_MODEL_FILE,
      "--text",
      TEXT,
      "--threshold",
      String(THRESHOLD),
    ],
    { encoding: "utf-8" },
  );
  return JSON.parse(raw.trim().split("\n").at(-1));
}

function int64Tensor2D(values) {
  const rows = values.length;
  const cols = values[0]?.length ?? 0;
  const data = BigInt64Array.from(values.flat().map((value) => BigInt(value)));
  return new ort.Tensor("int64", data, [rows, cols]);
}

function int64Tensor1D(values) {
  const data = BigInt64Array.from(values.map((value) => BigInt(value)));
  return new ort.Tensor("int64", data, [values.length]);
}

function reshapeSpanScores(flat, dims) {
  const [, labelCount, wordCount, widthCount] = dims;
  const scores = [];
  let offset = 0;
  for (let labelIndex = 0; labelIndex < labelCount; labelIndex += 1) {
    const labelRows = [];
    for (let wordIndex = 0; wordIndex < wordCount; wordIndex += 1) {
      labelRows.push(Array.from(flat.slice(offset, offset + widthCount)));
      offset += widthCount;
    }
    scores.push(labelRows);
  }
  return scores;
}

async function main() {
  const useLocalModel = existsSync(localModelDir);
  const modelPath = useLocalModel ? localModelDir : MODEL_ID;
  const onnxPath = useLocalModel ? join(localModelDir, ONNX_MODEL_FILE) : ONNX_MODEL_FILE;
  const preprocessPath = useLocalModel ? localModelDir : repoRoot;

  console.log(`[transformers.js] loading tokenizer from ${modelPath}`);
  const tokenizer = await AutoTokenizer.from_pretrained(modelPath);
  const encoded = await tokenizer(TEXT, { add_special_tokens: false });
  console.log(`[transformers.js] tokenizer tokens=${encoded.input_ids?.length ?? encoded.input_ids?.data?.length ?? "n/a"}`);

  console.log("[transformers.js] building GLiNER2 schema feed");
  const prepared = buildFeedWithPython(preprocessPath);

  console.log(`[transformers.js] loading ONNX session ${onnxPath}`);
  const session = await ort.InferenceSession.create(onnxPath, {
    executionProviders: ["cpu"],
  });

  const feed = prepared.feed;
  const outputs = await session.run({
    input_ids: int64Tensor2D(feed.input_ids),
    attention_mask: int64Tensor2D(feed.attention_mask),
    text_word_indices: int64Tensor2D(feed.text_word_indices),
    text_word_counts: int64Tensor1D(feed.text_word_counts),
  });

  const entities = decodeEntitySpanScores({
    text: prepared.text,
    labels: prepared.labels,
    countLogits: Array.from(outputs.count_logits.data),
    spanScores: reshapeSpanScores(Array.from(outputs.span_scores.data), outputs.span_scores.dims),
    startMapping: prepared.start_mapping,
    endMapping: prepared.end_mapping,
    threshold: THRESHOLD,
  });

  console.log(JSON.stringify({ entities, reference: prepared.predictions }, null, 2));
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
