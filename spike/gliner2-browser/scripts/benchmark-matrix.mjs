#!/usr/bin/env node
/**
 * GLiNER2 Browser Benchmark Matrix
 *
 * Measures download size, load time, inference latency, memory, and accuracy
 * for multiple GLiNER2 model variants.
 *
 * Usage:
 *   node scripts/benchmark-matrix.mjs [--models all|<model-id>] [--output results.json]
 *
 * Add new candidates to MODEL_CANDIDATES array below.
 */

import { existsSync, mkdirSync, readFileSync, writeFileSync, statSync } from "node:fs";
import { execSync, execFileSync, spawn } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";
import * as ort from "onnxruntime-node";
import { Gliner2Preprocessor } from "../src/gliner2-preprocessor.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const ARTIFACTS_DIR = resolve(ROOT, "artifacts");
const RESULTS_DIR = resolve(ROOT, "results");
const FIXTURES_DIR = resolve(ROOT, "fixtures");

const MODEL_CANDIDATES = [
  {
    id: "okasi/gliner2-privacy-filter-pii-multi-onnx",
    variant: "q8",
    files: ["model_q8.onnx", "model_q8.onnx.data", "tokenizer.json", "tokenizer_config.json", "entity_labels.json", "model_q8_metadata.json", "config.json"],
    onnxModel: "model_q8.onnx",
    dataFile: "model_q8.onnx.data",
    description: "okasi Q8 export of fastino/gliner2-privacy-filter-PII-multi (Rust core pinned model)",
    isRustCoreModel: true,
    labels: ["name", "address", "email", "phone_num", "id_num", "url", "username"],
  },
  {
    id: "okasi/gliner2-privacy-filter-pii-multi-onnx",
    variant: "q4",
    files: ["model_q4.onnx", "model_q4.onnx.data", "tokenizer.json", "tokenizer_config.json", "entity_labels.json", "model_q4_metadata.json", "config.json"],
    onnxModel: "model_q4.onnx",
    dataFile: "model_q4.onnx.data",
    description: "okasi Q4 export of fastino/gliner2-privacy-filter-PII-multi",
    isRustCoreModel: true,
    labels: ["name", "address", "email", "phone_num", "id_num", "url", "username"],
  },
  {
    id: "onnx-community/GLiNER2.5-Decide-ONNX",
    variant: "q4f16",
    files: ["onnx/model_q4f16.onnx", "onnx/model_q4f16.onnx_data", "tokenizer.json", "tokenizer_config.json", "config.json"],
    onnxModel: "onnx/model_q4f16.onnx",
    dataFile: "onnx/model_q4f16.onnx_data",
    description: "onnx-community GLiNER2.5-Decide Q4F16 (499 MB, smaller full model)",
    isRustCoreModel: false,
    labels: ["name", "address", "email", "phone", "id", "url", "username"],
  },
  {
    id: "onnx-community/GLiNER2.5-Decide-mobile-ONNX",
    variant: "q4f16",
    files: ["onnx/model_q4f16.onnx", "onnx/model_q4f16.onnx_data", "tokenizer.json", "tokenizer_config.json", "config.json"],
    onnxModel: "onnx/model_q4f16.onnx",
    dataFile: "onnx/model_q4f16.onnx_data",
    description: "onnx-community GLiNER2.5-Decide-mobile Q4F16 (329 MB, smallest variant)",
    isRustCoreModel: false,
    labels: ["name", "address", "email", "phone", "id", "url", "username"],
  },
];

const LABEL_MAPPING = {
  name: "PERSON",
  address: "STREET_ADDRESS",
  phone: "PHONE",
  phone_num: "PHONE",
};

function parseArgs() {
  const args = process.argv.slice(2);
  const options = {
    models: "all",
    output: join(RESULTS_DIR, "benchmark-matrix.json"),
    skipDownload: false,
    verbose: false,
    warmupRuns: 2,
    benchRuns: 5,
  };

  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--models" && args[i + 1]) {
      options.models = args[++i];
    } else if (args[i] === "--output" && args[i + 1]) {
      options.output = args[++i];
    } else if (args[i] === "--skip-download") {
      options.skipDownload = true;
    } else if (args[i] === "--verbose" || args[i] === "-v") {
      options.verbose = true;
    } else if (args[i] === "--help" || args[i] === "-h") {
      console.log(`
GLiNER2 Browser Benchmark Matrix

Usage:
  node scripts/benchmark-matrix.mjs [options]

Options:
  --models <id|all>    Model to benchmark (default: all)
  --output <path>      Output JSON file (default: results/benchmark-matrix.json)
  --skip-download      Skip model download, use cached artifacts
  --verbose, -v        Verbose output
  --help, -h           Show this help

Available models:
${MODEL_CANDIDATES.map((m) => `  - ${m.id}/${m.variant}: ${m.description}`).join("\n")}
`);
      process.exit(0);
    }
  }

  return options;
}

async function downloadModel(model, targetDir) {
  const modelDir = join(targetDir, model.id.replace("/", "__"), model.variant);
  mkdirSync(modelDir, { recursive: true });

  const baseUrl = `https://huggingface.co/${model.id}/resolve/main`;

  for (const file of model.files) {
    const fileName = file.split("/").pop();
    const targetPath = join(modelDir, fileName);
    if (existsSync(targetPath)) {
      console.log(`  [skip] ${file} already exists`);
      continue;
    }

    const url = `${baseUrl}/${file}`;
    console.log(`  [download] ${file}...`);

    try {
      execSync(`curl -L --progress-bar -o "${targetPath}" "${url}"`, {
        stdio: ["ignore", "inherit", "inherit"],
        timeout: 600000,
      });
    } catch (error) {
      console.error(`  [error] Failed to download ${file}: ${error.message}`);
      throw error;
    }
  }

  return modelDir;
}

function getFileSizes(modelDir, model) {
  const sizes = {};
  let totalBytes = 0;

  for (const file of model.files) {
    const fileName = file.split("/").pop();
    const filePath = join(modelDir, fileName);
    if (existsSync(filePath)) {
      const stat = statSync(filePath);
      sizes[fileName] = stat.size;
      totalBytes += stat.size;
    }
  }

  return {
    files: sizes,
    totalBytes,
    totalMB: (totalBytes / (1024 * 1024)).toFixed(2),
  };
}

function loadFixtures() {
  const d4Path = join(FIXTURES_DIR, "d4-names-addresses.jsonl");
  if (!existsSync(d4Path)) {
    console.warn("[warn] D4 fixtures not found at", d4Path);
    return { d4: [] };
  }

  const lines = readFileSync(d4Path, "utf-8").trim().split("\n");
  const d4 = lines.map((line) => JSON.parse(line));

  return { d4 };
}

function calculateMetrics(predictions, groundTruth, labelMap) {
  const byLabel = {};
  const overall = { tp: 0, fp: 0, fn: 0 };

  const predCopy = predictions.map((p) => ({ ...p, _matched: false }));

  for (const gt of groundTruth) {
    const gtLabel = gt.label;
    const gtMapped = LABEL_MAPPING[gtLabel] || gtLabel.toUpperCase();

    if (!byLabel[gtMapped]) {
      byLabel[gtMapped] = { tp: 0, fp: 0, fn: 0 };
    }

    const match = predCopy.find((p) => {
      if (p._matched) return false;
      const pMapped = LABEL_MAPPING[p.label] || p.label.toUpperCase();
      return pMapped === gtMapped && p.start === gt.start && p.end === gt.end;
    });

    if (match) {
      byLabel[gtMapped].tp++;
      overall.tp++;
      match._matched = true;
    } else {
      byLabel[gtMapped].fn++;
      overall.fn++;
    }
  }

  for (const pred of predCopy) {
    if (pred._matched) continue;
    const pMapped = LABEL_MAPPING[pred.label] || pred.label.toUpperCase();
    if (!byLabel[pMapped]) {
      byLabel[pMapped] = { tp: 0, fp: 0, fn: 0 };
    }
    byLabel[pMapped].fp++;
    overall.fp++;
  }

  const compute = (stats) => {
    const precision = stats.tp + stats.fp > 0 ? stats.tp / (stats.tp + stats.fp) : 0;
    const recall = stats.tp + stats.fn > 0 ? stats.tp / (stats.tp + stats.fn) : 0;
    const f1 = precision + recall > 0 ? (2 * precision * recall) / (precision + recall) : 0;
    return { precision, recall, f1, ...stats };
  };

  const result = {
    overall: compute(overall),
    byLabel: {},
  };

  for (const [label, stats] of Object.entries(byLabel)) {
    result.byLabel[label] = compute(stats);
  }

  return result;
}

async function benchmarkModel(modelDir, model, fixtures, options) {
  const onnxPath = join(modelDir, model.onnxModel.split("/").pop());
  const dataPath = model.dataFile ? join(modelDir, model.dataFile.split("/").pop()) : null;
  const tokenizerPath = modelDir;

  const results = {
    coldLoadTimeMs: null,
    warmLoadTimeMs: null,
    warmLatencyMs: [],
    predictions: [],
    metrics: null,
    peakMemoryMB: null,
    errors: [],
  };

  try {
    console.log("  Loading preprocessor...");
    const preprocessor = await Gliner2Preprocessor.fromPretrained(tokenizerPath, { localOnly: true });

    const sessionOptions = {
      executionProviders: ["cpu"],
      graphOptimizationLevel: "all",
    };

    // Note: onnxruntime-node auto-resolves external data files when in same directory
    // Explicitly passing externalData with buffer can cause issues in some versions

    console.log("  Measuring cold load time...");
    const coldStart = performance.now();
    const session = await ort.InferenceSession.create(onnxPath, sessionOptions);
    results.coldLoadTimeMs = performance.now() - coldStart;

    const memUsage = process.memoryUsage();
    results.peakMemoryMB = parseFloat((memUsage.heapUsed / (1024 * 1024)).toFixed(2));

    console.log("  Measuring warm load time...");
    const warmStart = performance.now();
    const session2 = await ort.InferenceSession.create(onnxPath, sessionOptions);
    results.warmLoadTimeMs = performance.now() - warmStart;
    await session2.release();

    const sample1kb = `Please contact John Smith regarding the quarterly report. He lives at 742 Evergreen Terrace, Springfield, IL 62704. His email is john.smith@example.com and phone is (555) 123-4567. SSN: 123-45-6789. The meeting with María García-López was rescheduled. Contact her at maria@company.org. Her office is at 456 Oak Avenue, Suite 200, Boston, MA 02108. Visit https://example.com/profile for more info. User ID: user_12345.`;

    console.log("  Running latency benchmark on 1KB sample...");
    for (let i = 0; i < options.warmupRuns; i++) {
      const prepared = preprocessor.prepareFeed(sample1kb, model.labels);
      const feed = {
        input_ids: new ort.Tensor("int64", BigInt64Array.from(prepared.feed.input_ids[0].map(BigInt)), [1, prepared.feed.input_ids[0].length]),
        attention_mask: new ort.Tensor("int64", BigInt64Array.from(prepared.feed.attention_mask[0].map(BigInt)), [1, prepared.feed.attention_mask[0].length]),
        text_word_indices: new ort.Tensor("int64", BigInt64Array.from(prepared.feed.text_word_indices[0].map(BigInt)), [1, prepared.feed.text_word_indices[0].length]),
        text_word_counts: new ort.Tensor("int64", BigInt64Array.from([BigInt(prepared.feed.text_word_counts[0])]), [1]),
      };
      await session.run(feed);
    }

    for (let i = 0; i < options.benchRuns; i++) {
      const prepared = preprocessor.prepareFeed(sample1kb, model.labels);
      const feed = {
        input_ids: new ort.Tensor("int64", BigInt64Array.from(prepared.feed.input_ids[0].map(BigInt)), [1, prepared.feed.input_ids[0].length]),
        attention_mask: new ort.Tensor("int64", BigInt64Array.from(prepared.feed.attention_mask[0].map(BigInt)), [1, prepared.feed.attention_mask[0].length]),
        text_word_indices: new ort.Tensor("int64", BigInt64Array.from(prepared.feed.text_word_indices[0].map(BigInt)), [1, prepared.feed.text_word_indices[0].length]),
        text_word_counts: new ort.Tensor("int64", BigInt64Array.from([BigInt(prepared.feed.text_word_counts[0])]), [1]),
      };
      const start = performance.now();
      await session.run(feed);
      results.warmLatencyMs.push(performance.now() - start);
    }

    console.log("  Running accuracy evaluation on D4 fixtures...");
    const allPredictions = [];

    for (const fixture of fixtures.d4) {
      try {
        const prepared = preprocessor.prepareFeed(fixture.text, model.labels);
        const feed = {
          input_ids: new ort.Tensor("int64", BigInt64Array.from(prepared.feed.input_ids[0].map(BigInt)), [1, prepared.feed.input_ids[0].length]),
          attention_mask: new ort.Tensor("int64", BigInt64Array.from(prepared.feed.attention_mask[0].map(BigInt)), [1, prepared.feed.attention_mask[0].length]),
          text_word_indices: new ort.Tensor("int64", BigInt64Array.from(prepared.feed.text_word_indices[0].map(BigInt)), [1, prepared.feed.text_word_indices[0].length]),
          text_word_counts: new ort.Tensor("int64", BigInt64Array.from([BigInt(prepared.feed.text_word_counts[0])]), [1]),
        };
        const outputs = await session.run(feed);

        const entities = decodeSpans(outputs, prepared, fixture.text, model.labels, 0.5);

        allPredictions.push({
          id: fixture.id,
          text: fixture.text,
          predictions: entities,
          groundTruth: fixture.entities,
        });
      } catch (error) {
        if (options.verbose) {
          console.error(`    Error on fixture ${fixture.id}: ${error.message}`);
        }
      }
    }

    const allGroundTruth = fixtures.d4.flatMap((f) => f.entities.map((e) => ({ ...e, fixtureId: f.id })));
    const allPreds = allPredictions.flatMap((p) => p.predictions.map((e) => ({ ...e, fixtureId: p.id })));

    results.metrics = calculateMetrics(allPreds, allGroundTruth, LABEL_MAPPING);
    results.predictions = allPredictions;

    await session.release();
  } catch (error) {
    results.errors.push(error.message);
    console.error(`  Error: ${error.message}`);
  }

  return results;
}

function decodeSpans(outputs, prepared, text, labels, threshold) {
  const spanScores = outputs.span_scores?.data;
  const countLogits = outputs.count_logits?.data;

  if (!spanScores) return [];

  let countPred = 0;
  if (countLogits) {
    const logitsArr = Array.from(countLogits);
    countPred = logitsArr.indexOf(Math.max(...logitsArr));
  }
  if (countPred === 0) countPred = 1;

  const dims = outputs.span_scores.dims;
  const [, numLabels, numWords, maxSpanLen] = dims;

  const entities = [];
  const startMapping = prepared.start_mapping;
  const endMapping = prepared.end_mapping;

  for (let labelIdx = 0; labelIdx < numLabels && labelIdx < labels.length; labelIdx++) {
    const label = labels[labelIdx];

    for (let wordIdx = 0; wordIdx < numWords && wordIdx < prepared.words.length; wordIdx++) {
      for (let spanLen = 0; spanLen < maxSpanLen; spanLen++) {
        const idx = (labelIdx * numWords * maxSpanLen) + (wordIdx * maxSpanLen) + spanLen;
        const score = spanScores[idx];

        if (score >= threshold) {
          const endWordIdx = wordIdx + spanLen;
          if (endWordIdx < prepared.words.length) {
            const charStart = startMapping[wordIdx];
            const charEnd = endMapping[endWordIdx];
            const matchedText = text.slice(charStart, charEnd);

            if (matchedText.trim()) {
              entities.push({
                label,
                text: matchedText,
                start: charStart,
                end: charEnd,
                score,
              });
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

async function main() {
  const options = parseArgs();

  mkdirSync(ARTIFACTS_DIR, { recursive: true });
  mkdirSync(RESULTS_DIR, { recursive: true });

  const fixtures = loadFixtures();
  console.log(`Loaded ${fixtures.d4.length} D4 fixtures\n`);

  const candidates =
    options.models === "all"
      ? MODEL_CANDIDATES
      : MODEL_CANDIDATES.filter(
          (m) => `${m.id}/${m.variant}` === options.models || m.id === options.models
        );

  if (candidates.length === 0) {
    console.error(`No models matched: ${options.models}`);
    process.exit(1);
  }

  const results = {
    timestamp: new Date().toISOString(),
    platform: {
      node: process.version,
      arch: process.arch,
      platform: process.platform,
      cpus: os.cpus().length,
    },
    benchmarkConfig: {
      warmupRuns: options.warmupRuns,
      benchRuns: options.benchRuns,
      sample: "1KB text with mixed PII",
    },
    models: [],
  };

  for (const model of candidates) {
    console.log(`\n${"=".repeat(70)}`);
    console.log(`Benchmarking: ${model.id} (${model.variant})`);
    console.log(`Description: ${model.description}`);
    console.log(`${"=".repeat(70)}\n`);

    let modelDir;
    try {
      if (!options.skipDownload) {
        console.log("Downloading model files...");
        modelDir = await downloadModel(model, ARTIFACTS_DIR);
      } else {
        modelDir = join(ARTIFACTS_DIR, model.id.replace("/", "__"), model.variant);
        if (!existsSync(modelDir)) {
          console.error(`  Model directory not found: ${modelDir}`);
          continue;
        }
      }
    } catch (error) {
      console.error(`  Failed to download: ${error.message}`);
      results.models.push({
        id: model.id,
        variant: model.variant,
        error: `Download failed: ${error.message}`,
      });
      continue;
    }

    const sizes = getFileSizes(modelDir, model);
    console.log(`  Download size: ${sizes.totalMB} MB`);

    const bench = await benchmarkModel(modelDir, model, fixtures, options);

    const avgLatency = bench.warmLatencyMs.length > 0
      ? bench.warmLatencyMs.reduce((a, b) => a + b, 0) / bench.warmLatencyMs.length
      : null;

    const personRecall = bench.metrics?.byLabel?.PERSON?.recall ?? null;
    const personF1 = bench.metrics?.byLabel?.PERSON?.f1 ?? null;
    const addressRecall = bench.metrics?.byLabel?.STREET_ADDRESS?.recall ?? null;
    const addressF1 = bench.metrics?.byLabel?.STREET_ADDRESS?.f1 ?? null;
    const overallF1 = bench.metrics?.overall?.f1 ?? null;

    const clearsADR004 = personRecall !== null && personRecall >= 0.9;

    const modelResult = {
      id: model.id,
      variant: model.variant,
      description: model.description,
      isRustCoreModel: model.isRustCoreModel,
      downloadSizeMB: parseFloat(sizes.totalMB),
      downloadSizeBytes: sizes.totalBytes,
      fileSizes: sizes.files,
      coldLoadTimeMs: bench.coldLoadTimeMs ? parseFloat(bench.coldLoadTimeMs.toFixed(1)) : null,
      warmLoadTimeMs: bench.warmLoadTimeMs ? parseFloat(bench.warmLoadTimeMs.toFixed(1)) : null,
      warmLatencyAvgMs: avgLatency ? parseFloat(avgLatency.toFixed(1)) : null,
      warmLatencyAllMs: bench.warmLatencyMs.map((l) => parseFloat(l.toFixed(1))),
      peakMemoryMB: bench.peakMemoryMB,
      metrics: {
        overall: bench.metrics?.overall ?? null,
        byLabel: bench.metrics?.byLabel ?? null,
      },
      personRecall: personRecall ? parseFloat(personRecall.toFixed(4)) : null,
      personF1: personF1 ? parseFloat(personF1.toFixed(4)) : null,
      addressRecall: addressRecall ? parseFloat(addressRecall.toFixed(4)) : null,
      addressF1: addressF1 ? parseFloat(addressF1.toFixed(4)) : null,
      overallF1: overallF1 ? parseFloat(overallF1.toFixed(4)) : null,
      clearsADR004: clearsADR004,
      errors: bench.errors,
    };

    results.models.push(modelResult);

    console.log(`\n  Results:`);
    console.log(`    Cold load:     ${modelResult.coldLoadTimeMs ?? "N/A"} ms`);
    console.log(`    Warm latency:  ${modelResult.warmLatencyAvgMs ?? "N/A"} ms`);
    console.log(`    Peak memory:   ${modelResult.peakMemoryMB ?? "N/A"} MB`);
    console.log(`    PERSON recall: ${modelResult.personRecall ?? "N/A"} ${clearsADR004 ? "✓ (clears ADR-004)" : "✗"}`);
    console.log(`    PERSON F1:     ${modelResult.personF1 ?? "N/A"}`);
    console.log(`    Overall F1:    ${modelResult.overallF1 ?? "N/A"}`);
  }

  writeFileSync(options.output, JSON.stringify(results, null, 2));
  console.log(`\nResults written to: ${options.output}`);

  console.log("\n" + "=".repeat(100));
  console.log("BENCHMARK MATRIX SUMMARY");
  console.log("=".repeat(100));

  const header = [
    "Model".padEnd(45),
    "Size".padStart(8),
    "Cold".padStart(8),
    "Warm".padStart(8),
    "P-Recall".padStart(9),
    "P-F1".padStart(8),
    "ADR-004".padStart(8),
  ].join(" | ");

  console.log(header);
  console.log("-".repeat(header.length));

  for (const m of results.models) {
    const row = [
      `${m.id.split("/").pop()}/${m.variant}`.slice(0, 45).padEnd(45),
      m.downloadSizeMB ? `${m.downloadSizeMB}MB`.padStart(8) : "N/A".padStart(8),
      m.coldLoadTimeMs ? `${(m.coldLoadTimeMs / 1000).toFixed(1)}s`.padStart(8) : "N/A".padStart(8),
      m.warmLatencyAvgMs ? `${m.warmLatencyAvgMs.toFixed(0)}ms`.padStart(8) : "N/A".padStart(8),
      m.personRecall ? `${(m.personRecall * 100).toFixed(1)}%`.padStart(9) : "N/A".padStart(9),
      m.personF1 ? `${(m.personF1 * 100).toFixed(1)}%`.padStart(8) : "N/A".padStart(8),
      m.clearsADR004 ? "✓".padStart(8) : "✗".padStart(8),
    ].join(" | ");
    console.log(row);
  }

  console.log("\nNotes:");
  console.log("  - Cold load = first session creation including model loading");
  console.log("  - Warm latency = average inference time on 1KB text (after warmup)");
  console.log("  - ADR-004 requires ≥90% PERSON recall");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
