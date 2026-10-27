#!/usr/bin/env node
/**
 * Run threshold sweep on browser ONNX inference to compare with native.
 */
import { existsSync, readFileSync, writeFileSync, mkdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import * as ort from "onnxruntime-node";
import { Gliner2Preprocessor } from "../src/gliner2-preprocessor.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const ARTIFACTS_DIR = resolve(ROOT, "artifacts");
const FIXTURES_DIR = resolve(ROOT, "fixtures");

const THRESHOLDS = [0.3, 0.4, 0.5];
const MODEL = {
  id: "okasi/gliner2-privacy-filter-pii-multi-onnx",
  variant: "q8",
  onnxModel: "model_q8.onnx",
  dataFile: "model_q8.onnx.data",
  labels: ["name", "address", "email", "phone_num", "id_num", "url", "username"],
};

const LABEL_MAPPING = {
  name: "PERSON",
  address: "STREET_ADDRESS",
  phone_num: "PHONE",
};

function loadFixtures() {
  const d4Path = join(FIXTURES_DIR, "d4-names-addresses.jsonl");
  const lines = readFileSync(d4Path, "utf-8").trim().split("\n");
  return lines.map((line) => JSON.parse(line));
}

function decodeSpans(outputs, prepared, text, labels, threshold) {
  const spanScores = outputs.span_scores.data;
  const dims = outputs.span_scores.dims;
  const [, numLabels, numWords, maxSpanLen] = dims;
  const entities = [];

  for (let labelIdx = 0; labelIdx < numLabels && labelIdx < labels.length; labelIdx++) {
    const label = labels[labelIdx];
    for (let wordIdx = 0; wordIdx < numWords && wordIdx < prepared.words.length; wordIdx++) {
      for (let spanLen = 0; spanLen < maxSpanLen; spanLen++) {
        const idx = labelIdx * numWords * maxSpanLen + wordIdx * maxSpanLen + spanLen;
        const score = spanScores[idx];
        if (score >= threshold) {
          const endWordIdx = wordIdx + spanLen;
          if (endWordIdx < prepared.words.length) {
            const charStart = prepared.start_mapping[wordIdx];
            const charEnd = prepared.end_mapping[endWordIdx];
            const matchedText = text.slice(charStart, charEnd);
            if (matchedText.trim()) {
              entities.push({ label, text: matchedText, start: charStart, end: charEnd, score });
            }
          }
        }
      }
    }
  }

  entities.sort((a, b) => b.score - a.score);
  const kept = [];
  for (const entity of entities) {
    const overlaps = kept.some((e) => !(entity.end <= e.start || entity.start >= e.end));
    if (!overlaps) kept.push(entity);
  }
  kept.sort((a, b) => a.start - b.start);
  return kept;
}

function calculateMetrics(predictions, fixtures) {
  const stats = {};
  const missed = [];

  for (let i = 0; i < predictions.length; i++) {
    const preds = predictions[i];
    const fixture = fixtures[i];
    const gt = fixture.entities || [];

    const predMatched = preds.map(() => false);

    for (const gtEntity of gt) {
      const gtLabel = LABEL_MAPPING[gtEntity.label] || gtEntity.label.toUpperCase();

      if (!stats[gtLabel]) {
        stats[gtLabel] = { tp: 0, fp: 0, fn: 0 };
      }

      let matched = false;
      for (let j = 0; j < preds.length; j++) {
        if (predMatched[j]) continue;
        const pred = preds[j];
        const predLabel = LABEL_MAPPING[pred.label] || pred.label.toUpperCase();
        if (predLabel === gtLabel && pred.start === gtEntity.start && pred.end === gtEntity.end) {
          stats[gtLabel].tp++;
          predMatched[j] = true;
          matched = true;
          break;
        }
      }

      if (!matched) {
        stats[gtLabel].fn++;
        missed.push({
          fixture_id: fixture.id,
          label: gtLabel,
          text: gtEntity.text || fixture.text.slice(gtEntity.start, gtEntity.end),
          start: gtEntity.start,
          end: gtEntity.end,
        });
      }
    }

    for (let j = 0; j < preds.length; j++) {
      if (!predMatched[j]) {
        const pred = preds[j];
        const predLabel = LABEL_MAPPING[pred.label] || pred.label.toUpperCase();
        if (!stats[predLabel]) {
          stats[predLabel] = { tp: 0, fp: 0, fn: 0 };
        }
        stats[predLabel].fp++;
      }
    }
  }

  for (const [label, s] of Object.entries(stats)) {
    const { tp, fp, fn } = s;
    s.precision = tp + fp > 0 ? tp / (tp + fp) : 0;
    s.recall = tp + fn > 0 ? tp / (tp + fn) : 0;
    s.f1 = s.precision + s.recall > 0 ? (2 * s.precision * s.recall) / (s.precision + s.recall) : 0;
  }

  return { stats, missed };
}

async function main() {
  const fixtures = loadFixtures();
  console.log(`Loaded ${fixtures.length} fixtures`);

  const modelDir = join(ARTIFACTS_DIR, MODEL.id.replace("/", "__"), MODEL.variant);
  const onnxPath = join(modelDir, MODEL.onnxModel);

  console.log("Loading preprocessor...");
  const preprocessor = await Gliner2Preprocessor.fromPretrained(modelDir, { localOnly: true });

  console.log("Loading ONNX model...");
  const session = await ort.InferenceSession.create(onnxPath, {
    executionProviders: ["cpu"],
    graphOptimizationLevel: "all",
  });

  const results = {};

  for (const threshold of THRESHOLDS) {
    console.log(`\n${"=".repeat(60)}`);
    console.log(`Threshold: ${threshold}`);
    console.log("=".repeat(60));

    const predictions = [];

    for (const fixture of fixtures) {
      const prepared = preprocessor.prepareFeed(fixture.text, MODEL.labels);
      const feed = {
        input_ids: new ort.Tensor(
          "int64",
          BigInt64Array.from(prepared.feed.input_ids[0].map(BigInt)),
          [1, prepared.feed.input_ids[0].length]
        ),
        attention_mask: new ort.Tensor(
          "int64",
          BigInt64Array.from(prepared.feed.attention_mask[0].map(BigInt)),
          [1, prepared.feed.attention_mask[0].length]
        ),
        text_word_indices: new ort.Tensor(
          "int64",
          BigInt64Array.from(prepared.feed.text_word_indices[0].map(BigInt)),
          [1, prepared.feed.text_word_indices[0].length]
        ),
        text_word_counts: new ort.Tensor(
          "int64",
          BigInt64Array.from([BigInt(prepared.feed.text_word_counts[0])]),
          [1]
        ),
      };

      const outputs = await session.run(feed);
      const entities = decodeSpans(outputs, prepared, fixture.text, MODEL.labels, threshold);
      predictions.push(entities);
    }

    const { stats, missed } = calculateMetrics(predictions, fixtures);

    console.log("\nResults:");
    for (const [label, m] of Object.entries(stats).sort()) {
      console.log(
        `  ${label}: P=${m.precision.toFixed(4)} R=${m.recall.toFixed(4)} F1=${m.f1.toFixed(4)} (TP=${m.tp} FP=${m.fp} FN=${m.fn})`
      );
    }

    results[threshold] = { stats, missed: threshold === 0.5 ? missed : [] };

    if (threshold === 0.5) {
      const personMissed = missed.filter((m) => m.label === "PERSON");
      console.log(`\nMissed PERSON entities (${personMissed.length} total):`);
      for (const m of personMissed) {
        console.log(`  [${m.fixture_id}] "${m.text}" @ ${m.start}-${m.end}`);
      }
    }
  }

  await session.release();

  const output = {
    runtime: "onnxruntime-node",
    model: MODEL.id,
    variant: MODEL.variant,
    labels: MODEL.labels,
    fixture_count: fixtures.length,
    results_by_threshold: results,
  };

  const outPath = join(ROOT, "results", "threshold-sweep-node.json");
  writeFileSync(outPath, JSON.stringify(output, null, 2));
  console.log(`\nResults written to ${outPath}`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
