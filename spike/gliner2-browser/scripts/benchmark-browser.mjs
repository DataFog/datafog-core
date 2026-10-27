#!/usr/bin/env node
/**
 * GLiNER2 Browser Benchmark
 *
 * Runs model benchmarks in headless Chromium via Playwright.
 * Measures: cold load, warm latency, memory, accuracy on D4 fixtures.
 */

import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync, writeFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execSync } from "node:child_process";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const ARTIFACTS_DIR = resolve(ROOT, "artifacts");
const RESULTS_DIR = resolve(ROOT, "results");
const FIXTURES_DIR = resolve(ROOT, "fixtures");

const MODEL_CANDIDATES = [
  {
    id: "okasi/gliner2-privacy-filter-pii-multi-onnx",
    variant: "q8",
    files: ["model_q8.onnx", "model_q8.onnx.data", "tokenizer.json", "tokenizer_config.json", "entity_labels.json"],
    onnxModel: "model_q8.onnx",
    dataFile: "model_q8.onnx.data",
    description: "okasi Q8 export",
    labels: ["name", "address", "email", "phone_num", "id_num", "url", "username"],
  },
  {
    id: "okasi/gliner2-privacy-filter-pii-multi-onnx",
    variant: "q4",
    files: ["model_q4.onnx", "model_q4.onnx.data", "tokenizer.json", "tokenizer_config.json", "entity_labels.json"],
    onnxModel: "model_q4.onnx",
    dataFile: "model_q4.onnx.data",
    description: "okasi Q4 export",
    labels: ["name", "address", "email", "phone_num", "id_num", "url", "username"],
  },
];

const LABEL_MAPPING = {
  name: "PERSON",
  address: "STREET_ADDRESS",
  phone_num: "PHONE",
};

function parseArgs() {
  const args = process.argv.slice(2);
  const options = {
    models: "all",
    output: join(RESULTS_DIR, "benchmark-browser.json"),
    skipDownload: false,
    verbose: false,
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
    execSync(`curl -L --progress-bar -o "${targetPath}" "${url}"`, {
      stdio: ["ignore", "inherit", "inherit"],
      timeout: 600000,
    });
  }

  return modelDir;
}

function getFileSizes(modelDir, model) {
  let totalBytes = 0;
  const sizes = {};
  for (const file of model.files) {
    const fileName = file.split("/").pop();
    const filePath = join(modelDir, fileName);
    if (existsSync(filePath)) {
      const stat = statSync(filePath);
      sizes[fileName] = stat.size;
      totalBytes += stat.size;
    }
  }
  return { files: sizes, totalBytes, totalMB: (totalBytes / (1024 * 1024)).toFixed(2) };
}

function loadFixtures() {
  const d4Path = join(FIXTURES_DIR, "d4-names-addresses.jsonl");
  if (!existsSync(d4Path)) {
    return { d4: [] };
  }
  const lines = readFileSync(d4Path, "utf-8").trim().split("\n");
  return { d4: lines.map((line) => JSON.parse(line)) };
}

function createTestServer(modelDir) {
  const mimeTypes = {
    ".html": "text/html",
    ".js": "text/javascript",
    ".mjs": "text/javascript",
    ".json": "application/json",
    ".onnx": "application/octet-stream",
    ".data": "application/octet-stream",
    ".wasm": "application/wasm",
  };

  const server = createServer((req, res) => {
    let filePath = req.url === "/" ? "/index.html" : req.url;
    filePath = filePath.split("?")[0];

    let fullPath;
    if (filePath.startsWith("/node_modules/")) {
      fullPath = join(ROOT, filePath);
    } else if (filePath.startsWith("/model/")) {
      fullPath = join(modelDir, filePath.replace("/model/", ""));
    } else {
      fullPath = join(ROOT, "public", filePath);
    }

    if (!existsSync(fullPath)) {
      res.writeHead(404);
      res.end("Not found: " + filePath);
      return;
    }

    const ext = filePath.match(/\.[^.]+$/)?.[0] || "";
    const contentType = mimeTypes[ext] || "application/octet-stream";

    res.writeHead(200, {
      "Content-Type": contentType,
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    });
    res.end(readFileSync(fullPath));
  });

  return server;
}

function calculateMetrics(predictions, groundTruth) {
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

  const result = { overall: compute(overall), byLabel: {} };
  for (const [label, stats] of Object.entries(byLabel)) {
    result.byLabel[label] = compute(stats);
  }
  return result;
}

async function benchmarkInBrowser(modelDir, model, fixtures, options) {
  const server = createTestServer(modelDir);
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = server.address().port;
  const baseUrl = `http://127.0.0.1:${port}`;

  console.log(`  Server running on ${baseUrl}`);

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext();
  const page = await context.newPage();

  page.on("console", (msg) => {
    if (options.verbose) {
      console.log(`  [browser] ${msg.type()}: ${msg.text()}`);
    }
  });

  page.on("pageerror", (err) => {
    console.error(`  [browser error] ${err.message}`);
  });

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
    const testHtml = `
<!DOCTYPE html>
<html>
<head>
  <title>GLiNER2 Browser Benchmark</title>
</head>
<body>
  <div id="status">Loading...</div>
  <script type="module">
    import { AutoTokenizer, env } from '/node_modules/@huggingface/transformers/dist/transformers.min.js';
    const ort = await import('/node_modules/onnxruntime-web/dist/ort.all.min.mjs');

    window.ort = ort;
    window.benchmarkResults = {};

    ort.env.wasm.wasmPaths = '/node_modules/onnxruntime-web/dist/';
    env.allowRemoteModels = false;
    env.allowLocalModels = true;

    class Gliner2Preprocessor {
      constructor(tokenizer, entityLabels) {
        this.tokenizer = tokenizer;
        this.entityLabels = entityLabels;
      }

      static async create(modelPath, labels) {
        const tokenizer = await AutoTokenizer.from_pretrained(modelPath, {
          local_files_only: true,
        });
        return new Gliner2Preprocessor(tokenizer, labels);
      }

      splitIntoWords(text) {
        const words = [];
        const pattern = /\\S+/g;
        let match;

        while ((match = pattern.exec(text)) !== null) {
          const token = match[0];
          const tokenStart = match.index;

          let leadingPunct = "";
          let trailingPunct = "";
          let core = token;

          while (core.length > 0 && /[.,!?;:'\"\\-–—()\\[\\]{}]/.test(core[0])) {
            leadingPunct += core[0];
            core = core.slice(1);
          }

          while (core.length > 0 && /[.,!?;:'\"\\-–—()\\[\\]{}]/.test(core[core.length - 1])) {
            trailingPunct = core[core.length - 1] + trailingPunct;
            core = core.slice(0, -1);
          }

          let offset = tokenStart;

          for (const ch of leadingPunct) {
            words.push({ word: ch, start: offset, end: offset + 1 });
            offset++;
          }

          if (core.length > 0) {
            words.push({ word: core, start: offset, end: offset + core.length });
            offset += core.length;
          }

          for (const ch of trailingPunct) {
            words.push({ word: ch, start: offset, end: offset + 1 });
            offset++;
          }
        }

        return words;
      }

      encodeWord(word) {
        const encoded = this.tokenizer(word, {
          add_special_tokens: false,
          return_tensors: false,
        });
        const data = encoded.input_ids.data || encoded.input_ids;
        return Array.from(data).map(Number);
      }

      prepareInputs(text, labels = null) {
        const activeLabels = labels || this.entityLabels;
        const words = this.splitIntoWords(text);

        const schemaText = ' ( [P] entities ( ' + activeLabels.map(l => '[E] ' + l).join(' ') + ' ) ) [SEP_TEXT]';

        const schemaEncoded = this.tokenizer(schemaText, {
          add_special_tokens: false,
          return_tensors: false,
        });
        const schemaIds = Array.from(schemaEncoded.input_ids.data || schemaEncoded.input_ids).map(Number);

        const textTokens = [];
        const wordIndices = [];
        const startMapping = [];
        const endMapping = [];

        for (const { word, start, end } of words) {
          wordIndices.push(schemaIds.length + textTokens.length);
          startMapping.push(start);
          endMapping.push(end);

          const wordWithSpace = ' ' + word.toLowerCase();
          const wordTokens = this.encodeWord(wordWithSpace);
          textTokens.push(...wordTokens);
        }

        const inputIds = [...schemaIds, ...textTokens];
        const attentionMask = new Array(inputIds.length).fill(1);

        return {
          inputIds,
          attentionMask,
          textWordIndices: wordIndices,
          textWordCounts: words.length,
          startMapping,
          endMapping,
          words,
          labels: activeLabels,
        };
      }
    }

    window.runBenchmark = async function(modelPath, dataPath, labels, fixtures, sample1kb) {
      const results = {
        coldLoadTimeMs: null,
        warmLoadTimeMs: null,
        warmLatencyMs: [],
        predictions: [],
        errors: [],
      };

      try {
        // Initialize preprocessor using @huggingface/transformers
        console.log('[INFO] Loading tokenizer with @huggingface/transformers...');
        const preprocessor = await Gliner2Preprocessor.create('/model', labels);
        console.log('[INFO] Tokenizer loaded');

        const coldStart = performance.now();
        const [modelResp, dataResp] = await Promise.all([
          fetch(modelPath),
          fetch(dataPath)
        ]);
        const modelBuffer = await modelResp.arrayBuffer();
        const dataBuffer = await dataResp.arrayBuffer();

        const sessionOptions = {
          executionProviders: ['wasm'],
          graphOptimizationLevel: 'all',
          externalData: [{
            path: dataPath.split('/').pop(),
            data: new Uint8Array(dataBuffer)
          }]
        };

        const session = await ort.InferenceSession.create(modelBuffer, sessionOptions);
        results.coldLoadTimeMs = performance.now() - coldStart;

        const warmStart = performance.now();
        const session2 = await ort.InferenceSession.create(modelBuffer, sessionOptions);
        results.warmLoadTimeMs = performance.now() - warmStart;
        await session2.release();

        function decodeSpans(outputs, prepared, text, labels, threshold) {
          const spanScores = outputs.span_scores.data;
          const dims = outputs.span_scores.dims;
          const [, numLabels, numWords, maxSpanLen] = dims;
          const entities = [];

          for (let labelIdx = 0; labelIdx < numLabels && labelIdx < labels.length; labelIdx++) {
            const label = labels[labelIdx];
            for (let wordIdx = 0; wordIdx < numWords && wordIdx < prepared.words.length; wordIdx++) {
              for (let spanLen = 0; spanLen < maxSpanLen; spanLen++) {
                const idx = (labelIdx * numWords * maxSpanLen) + (wordIdx * maxSpanLen) + spanLen;
                const score = spanScores[idx];
                if (score >= threshold) {
                  const endWordIdx = wordIdx + spanLen;
                  if (endWordIdx < prepared.words.length) {
                    const charStart = prepared.startMapping[wordIdx];
                    const charEnd = prepared.endMapping[endWordIdx];
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

        // Warmup runs
        for (let i = 0; i < 2; i++) {
          const prepared = preprocessor.prepareInputs(sample1kb, labels);
          const feed = {
            input_ids: new ort.Tensor('int64', BigInt64Array.from(prepared.inputIds.map(BigInt)), [1, prepared.inputIds.length]),
            attention_mask: new ort.Tensor('int64', BigInt64Array.from(prepared.attentionMask.map(BigInt)), [1, prepared.attentionMask.length]),
            text_word_indices: new ort.Tensor('int64', BigInt64Array.from(prepared.textWordIndices.map(BigInt)), [1, prepared.textWordIndices.length]),
            text_word_counts: new ort.Tensor('int64', BigInt64Array.from([BigInt(prepared.textWordCounts)]), [1]),
          };
          await session.run(feed);
        }

        // Latency benchmark
        for (let i = 0; i < 5; i++) {
          const prepared = preprocessor.prepareInputs(sample1kb, labels);
          const feed = {
            input_ids: new ort.Tensor('int64', BigInt64Array.from(prepared.inputIds.map(BigInt)), [1, prepared.inputIds.length]),
            attention_mask: new ort.Tensor('int64', BigInt64Array.from(prepared.attentionMask.map(BigInt)), [1, prepared.attentionMask.length]),
            text_word_indices: new ort.Tensor('int64', BigInt64Array.from(prepared.textWordIndices.map(BigInt)), [1, prepared.textWordIndices.length]),
            text_word_counts: new ort.Tensor('int64', BigInt64Array.from([BigInt(prepared.textWordCounts)]), [1]),
          };
          const start = performance.now();
          await session.run(feed);
          results.warmLatencyMs.push(performance.now() - start);
        }

        // Fixture evaluation
        let fixtureCount = 0;
        for (const fixture of fixtures) {
          try {
            const prepared = preprocessor.prepareInputs(fixture.text, labels);
            const feed = {
              input_ids: new ort.Tensor('int64', BigInt64Array.from(prepared.inputIds.map(BigInt)), [1, prepared.inputIds.length]),
              attention_mask: new ort.Tensor('int64', BigInt64Array.from(prepared.attentionMask.map(BigInt)), [1, prepared.attentionMask.length]),
              text_word_indices: new ort.Tensor('int64', BigInt64Array.from(prepared.textWordIndices.map(BigInt)), [1, prepared.textWordIndices.length]),
              text_word_counts: new ort.Tensor('int64', BigInt64Array.from([BigInt(prepared.textWordCounts)]), [1]),
            };
            const outputs = await session.run(feed);
            
            // Debug first fixture
            if (fixtureCount === 0 && outputs.span_scores) {
              const data = outputs.span_scores.data;
              const dims = outputs.span_scores.dims;
              let maxScore = -Infinity;
              let minScore = Infinity;
              for (let i = 0; i < Math.min(data.length, 10000); i++) {
                if (data[i] > maxScore) maxScore = data[i];
                if (data[i] < minScore) minScore = data[i];
              }
              console.log('[DEBUG] span_scores dims:', JSON.stringify(dims));
              console.log('[DEBUG] span_scores sample range: min=' + minScore.toFixed(4) + ' max=' + maxScore.toFixed(4));
              console.log('[DEBUG] words count:', prepared.words.length);
              console.log('[DEBUG] labels count:', labels.length);
            }
            
            const entities = decodeSpans(outputs, prepared, fixture.text, labels, 0.5);
            
            if (fixtureCount === 0) {
              console.log('[DEBUG] First fixture entities found:', entities.length);
            }
            
            results.predictions.push({
              id: fixture.id,
              text: fixture.text,
              predictions: entities,
              groundTruth: fixture.entities,
            });
            fixtureCount++;
          } catch (e) {
            results.errors.push('Fixture ' + fixture.id + ': ' + e.message);
          }
        }

        await session.release();
      } catch (e) {
        results.errors.push(e.message);
        console.error('[ERROR]', e.message, e.stack);
      }

      return results;
    };

    document.getElementById('status').textContent = 'Ready';
  </script>
</body>
</html>`;

    writeFileSync(join(ROOT, "public", "benchmark.html"), testHtml);

    await page.goto(`${baseUrl}/benchmark.html`);
    await page.waitForFunction(() => document.getElementById("status").textContent === "Ready", { timeout: 30000 });

    const sample1kb = `Please contact John Smith regarding the quarterly report. He lives at 742 Evergreen Terrace, Springfield, IL 62704. His email is john.smith@example.com and phone is (555) 123-4567. SSN: 123-45-6789. The meeting with María García-López was rescheduled. Contact her at maria@company.org. Her office is at 456 Oak Avenue, Suite 200, Boston, MA 02108. Visit https://example.com/profile for more info. User ID: user_12345.`;

    const modelPath = `/model/${model.onnxModel}`;
    const dataPath = `/model/${model.dataFile}`;

    console.log("  Running browser benchmark...");
    const browserResults = await page.evaluate(
      async ({ modelPath, dataPath, labels, fixtures, sample1kb }) => {
        return await window.runBenchmark(modelPath, dataPath, labels, fixtures, sample1kb);
      },
      { modelPath, dataPath, labels: model.labels, fixtures: fixtures.d4, sample1kb }
    );

    results.coldLoadTimeMs = browserResults.coldLoadTimeMs;
    results.warmLoadTimeMs = browserResults.warmLoadTimeMs;
    results.warmLatencyMs = browserResults.warmLatencyMs;
    results.predictions = browserResults.predictions;
    results.errors = browserResults.errors;

    const memoryInfo = await page.evaluate(() => {
      if (performance.memory) {
        return { usedJSHeapSize: performance.memory.usedJSHeapSize };
      }
      return null;
    });

    if (memoryInfo) {
      results.peakMemoryMB = parseFloat((memoryInfo.usedJSHeapSize / (1024 * 1024)).toFixed(2));
    }

    if (results.predictions.length > 0) {
      const allGroundTruth = fixtures.d4.flatMap((f) => f.entities.map((e) => ({ ...e, fixtureId: f.id })));
      const allPreds = results.predictions.flatMap((p) => p.predictions.map((e) => ({ ...e, fixtureId: p.id })));
      results.metrics = calculateMetrics(allPreds, allGroundTruth);
    }

  } catch (error) {
    results.errors.push(error.message);
  } finally {
    await browser.close();
    server.close();
  }

  return results;
}

async function main() {
  const options = parseArgs();

  mkdirSync(ARTIFACTS_DIR, { recursive: true });
  mkdirSync(RESULTS_DIR, { recursive: true });
  mkdirSync(join(ROOT, "public"), { recursive: true });

  const fixtures = loadFixtures();
  console.log(`Loaded ${fixtures.d4.length} D4 fixtures\n`);

  const candidates =
    options.models === "all"
      ? MODEL_CANDIDATES
      : MODEL_CANDIDATES.filter(
          (m) => `${m.id}/${m.variant}` === options.models || m.id === options.models
        );

  const results = {
    timestamp: new Date().toISOString(),
    runtime: "headless Chromium (Playwright)",
    ortVersion: "1.31.1",
    backend: "wasm",
    models: [],
  };

  for (const model of candidates) {
    console.log(`\n${"=".repeat(70)}`);
    console.log(`Benchmarking: ${model.id} (${model.variant})`);
    console.log(`${"=".repeat(70)}\n`);

    let modelDir;
    try {
      if (!options.skipDownload) {
        console.log("Downloading model files...");
        modelDir = await downloadModel(model, ARTIFACTS_DIR);
      } else {
        modelDir = join(ARTIFACTS_DIR, model.id.replace("/", "__"), model.variant);
      }
    } catch (error) {
      console.error(`  Download failed: ${error.message}`);
      continue;
    }

    const sizes = getFileSizes(modelDir, model);
    console.log(`  Download size: ${sizes.totalMB} MB`);

    const bench = await benchmarkInBrowser(modelDir, model, fixtures, options);

    const avgLatency = bench.warmLatencyMs.length > 0
      ? bench.warmLatencyMs.reduce((a, b) => a + b, 0) / bench.warmLatencyMs.length
      : null;

    const personRecall = bench.metrics?.byLabel?.PERSON?.recall ?? null;
    const personF1 = bench.metrics?.byLabel?.PERSON?.f1 ?? null;
    const addressRecall = bench.metrics?.byLabel?.STREET_ADDRESS?.recall ?? null;
    const addressF1 = bench.metrics?.byLabel?.STREET_ADDRESS?.f1 ?? null;
    const overallF1 = bench.metrics?.overall?.f1 ?? null;

    const clearsADR004 = personRecall !== null && personRecall >= 0.9;

    results.models.push({
      id: model.id,
      variant: model.variant,
      description: model.description,
      downloadSizeMB: parseFloat(sizes.totalMB),
      coldLoadTimeMs: bench.coldLoadTimeMs ? parseFloat(bench.coldLoadTimeMs.toFixed(1)) : null,
      warmLoadTimeMs: bench.warmLoadTimeMs ? parseFloat(bench.warmLoadTimeMs.toFixed(1)) : null,
      warmLatencyAvgMs: avgLatency ? parseFloat(avgLatency.toFixed(1)) : null,
      warmLatencyAllMs: bench.warmLatencyMs.map((l) => parseFloat(l.toFixed(1))),
      peakMemoryMB: bench.peakMemoryMB,
      metrics: bench.metrics,
      personRecall: personRecall ? parseFloat(personRecall.toFixed(4)) : null,
      personF1: personF1 ? parseFloat(personF1.toFixed(4)) : null,
      addressRecall: addressRecall ? parseFloat(addressRecall.toFixed(4)) : null,
      addressF1: addressF1 ? parseFloat(addressF1.toFixed(4)) : null,
      overallF1: overallF1 ? parseFloat(overallF1.toFixed(4)) : null,
      clearsADR004,
      errors: bench.errors,
    });

    console.log(`\n  Results:`);
    console.log(`    Cold load:      ${bench.coldLoadTimeMs?.toFixed(0) ?? "N/A"} ms`);
    console.log(`    Warm latency:   ${avgLatency?.toFixed(1) ?? "N/A"} ms`);
    console.log(`    PERSON recall:  ${personRecall ? (personRecall * 100).toFixed(1) + "%" : "N/A"} ${clearsADR004 ? "✓" : "✗"}`);
    console.log(`    PERSON F1:      ${personF1 ? (personF1 * 100).toFixed(1) + "%" : "N/A"}`);
    console.log(`    Overall F1:     ${overallF1 ? (overallF1 * 100).toFixed(1) + "%" : "N/A"}`);
    if (bench.errors.length > 0) {
      console.log(`    Errors: ${bench.errors.length}`);
    }
  }

  writeFileSync(options.output, JSON.stringify(results, null, 2));
  console.log(`\nResults written to: ${options.output}`);

  console.log("\n" + "=".repeat(100));
  console.log("BROWSER BENCHMARK SUMMARY");
  console.log("=".repeat(100));

  const header = [
    "Model".padEnd(45),
    "Size".padStart(8),
    "Cold".padStart(10),
    "Warm".padStart(10),
    "P-Recall".padStart(9),
    "P-F1".padStart(8),
    "ADR-004".padStart(8),
  ].join(" | ");

  console.log(header);
  console.log("-".repeat(header.length));

  for (const m of results.models) {
    const row = [
      `${m.id.split("/").pop()}/${m.variant}`.slice(0, 45).padEnd(45),
      `${m.downloadSizeMB}MB`.padStart(8),
      m.coldLoadTimeMs ? `${(m.coldLoadTimeMs / 1000).toFixed(1)}s`.padStart(10) : "N/A".padStart(10),
      m.warmLatencyAvgMs ? `${m.warmLatencyAvgMs.toFixed(0)}ms`.padStart(10) : "N/A".padStart(10),
      m.personRecall ? `${(m.personRecall * 100).toFixed(1)}%`.padStart(9) : "N/A".padStart(9),
      m.personF1 ? `${(m.personF1 * 100).toFixed(1)}%`.padStart(8) : "N/A".padStart(8),
      m.clearsADR004 ? "✓".padStart(8) : "✗".padStart(8),
    ].join(" | ");
    console.log(row);
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
