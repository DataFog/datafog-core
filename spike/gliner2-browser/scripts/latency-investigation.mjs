#!/usr/bin/env node
/**
 * Investigate browser WASM latency bottlenecks:
 * - SharedArrayBuffer availability (multi-threading)
 * - SIMD support
 * - WebGPU availability
 * - Sequence length impact
 */

import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const ARTIFACTS_DIR = resolve(ROOT, "artifacts");

const MODEL = {
  id: "okasi/gliner2-privacy-filter-pii-multi-onnx",
  variant: "q8",
  onnxModel: "model_q8.onnx",
  dataFile: "model_q8.onnx.data",
  labels: ["name", "address", "email", "phone_num", "id_num", "url", "username"],
};

function createTestServer(modelDir, enableCOI) {
  const mimeTypes = {
    ".html": "text/html",
    ".js": "text/javascript",
    ".mjs": "text/javascript",
    ".json": "application/json",
    ".onnx": "application/octet-stream",
    ".data": "application/octet-stream",
    ".wasm": "application/wasm",
  };

  return createServer((req, res) => {
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

    const headers = {
      "Content-Type": contentType,
    };

    if (enableCOI) {
      headers["Cross-Origin-Opener-Policy"] = "same-origin";
      headers["Cross-Origin-Embedder-Policy"] = "require-corp";
    }

    res.writeHead(200, headers);
    res.end(readFileSync(fullPath));
  });
}

async function runLatencyTest(page, testName, config) {
  const result = await page.evaluate(async ({ testName, config }) => {
    const results = {
      testName,
      config,
      capabilities: {},
      timings: [],
      errors: [],
    };

    try {
      results.capabilities = {
        sharedArrayBuffer: typeof SharedArrayBuffer !== "undefined",
        webgpu: !!navigator.gpu,
        simd: await (async () => {
          try {
            return WebAssembly.validate(
              new Uint8Array([
                0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0,
                65, 0, 253, 15, 253, 98, 11,
              ])
            );
          } catch {
            return false;
          }
        })(),
        hardwareConcurrency: navigator.hardwareConcurrency,
        userAgent: navigator.userAgent,
      };

      const ort = await import("/node_modules/onnxruntime-web/dist/ort.all.min.mjs");
      ort.env.wasm.wasmPaths = "/node_modules/onnxruntime-web/dist/";

      if (config.numThreads) {
        ort.env.wasm.numThreads = config.numThreads;
      }

      const { AutoTokenizer, env } = await import(
        "/node_modules/@huggingface/transformers/dist/transformers.min.js"
      );
      env.allowRemoteModels = false;
      env.allowLocalModels = true;

      const tokenizer = await AutoTokenizer.from_pretrained("/model", { local_files_only: true });

      const [modelResp, dataResp] = await Promise.all([
        fetch("/model/" + config.onnxModel),
        fetch("/model/" + config.dataFile),
      ]);
      const modelBuffer = await modelResp.arrayBuffer();
      const dataBuffer = await dataResp.arrayBuffer();

      const sessionOptions = {
        executionProviders: config.executionProviders || ["wasm"],
        graphOptimizationLevel: "all",
        externalData: [
          {
            path: config.dataFile,
            data: new Uint8Array(dataBuffer),
          },
        ],
      };

      const loadStart = performance.now();
      const session = await ort.InferenceSession.create(modelBuffer, sessionOptions);
      results.loadTimeMs = performance.now() - loadStart;

      const labels = config.labels;
      const schemaText =
        " ( [P] entities ( " + labels.map((l) => "[E] " + l).join(" ") + " ) ) [SEP_TEXT]";

      function splitWords(text) {
        const words = [];
        const pattern = /\S+/g;
        let match;
        while ((match = pattern.exec(text)) !== null) {
          const token = match[0];
          const tokenStart = match.index;
          let core = token;
          let leadingPunct = "";
          let trailingPunct = "";
          while (core.length > 0 && /[.,!?;:'"\-–—()\[\]{}]/.test(core[0])) {
            leadingPunct += core[0];
            core = core.slice(1);
          }
          while (core.length > 0 && /[.,!?;:'"\-–—()\[\]{}]/.test(core[core.length - 1])) {
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

      function encodeWord(word) {
        const encoded = tokenizer(word, {
          add_special_tokens: false,
          return_tensors: false,
        });
        const data = encoded.input_ids.data || encoded.input_ids;
        return Array.from(data).map(Number);
      }

      function preprocess(text) {
        const schemaEncoded = tokenizer(schemaText, {
          add_special_tokens: false,
          return_tensors: false,
        });
        const schemaIds = Array.from(schemaEncoded.input_ids.data || schemaEncoded.input_ids).map(
          Number
        );

        const words = splitWords(text);
        const textTokens = [];
        const wordIndices = [];
        const startMapping = [];
        const endMapping = [];

        for (const { word, start, end } of words) {
          wordIndices.push(schemaIds.length + textTokens.length);
          startMapping.push(start);
          endMapping.push(end);
          const wordWithSpace = " " + word.toLowerCase();
          const wordTokens = encodeWord(wordWithSpace);
          textTokens.push(...wordTokens);
        }

        return {
          inputIds: [...schemaIds, ...textTokens],
          attentionMask: new Array(schemaIds.length + textTokens.length).fill(1),
          wordIndices,
          startMapping,
          endMapping,
          words,
          sequenceLength: schemaIds.length + textTokens.length,
        };
      }

      const testTexts = [
        "John Smith at 123 Main St.", // Short
        "Please contact John Smith regarding the quarterly report. His email is john@example.com.", // Medium
        config.testText, // From config (1KB sample)
      ];

      for (let i = 0; i < 2; i++) {
        const prepared = preprocess(config.testText);
        const feed = {
          input_ids: new ort.Tensor(
            "int64",
            BigInt64Array.from(prepared.inputIds.map(BigInt)),
            [1, prepared.inputIds.length]
          ),
          attention_mask: new ort.Tensor(
            "int64",
            BigInt64Array.from(prepared.attentionMask.map(BigInt)),
            [1, prepared.attentionMask.length]
          ),
          text_word_indices: new ort.Tensor(
            "int64",
            BigInt64Array.from(prepared.wordIndices.map(BigInt)),
            [1, prepared.wordIndices.length]
          ),
          text_word_counts: new ort.Tensor(
            "int64",
            BigInt64Array.from([BigInt(prepared.words.length)]),
            [1]
          ),
        };
        await session.run(feed);
      }

      for (const text of testTexts) {
        const timings = [];

        for (let i = 0; i < 5; i++) {
          const prepared = preprocess(text);

          const feedStart = performance.now();
          const feed = {
            input_ids: new ort.Tensor(
              "int64",
              BigInt64Array.from(prepared.inputIds.map(BigInt)),
              [1, prepared.inputIds.length]
            ),
            attention_mask: new ort.Tensor(
              "int64",
              BigInt64Array.from(prepared.attentionMask.map(BigInt)),
              [1, prepared.attentionMask.length]
            ),
            text_word_indices: new ort.Tensor(
              "int64",
              BigInt64Array.from(prepared.wordIndices.map(BigInt)),
              [1, prepared.wordIndices.length]
            ),
            text_word_counts: new ort.Tensor(
              "int64",
              BigInt64Array.from([BigInt(prepared.words.length)]),
              [1]
            ),
          };
          const feedTime = performance.now() - feedStart;

          const inferStart = performance.now();
          await session.run(feed);
          const inferTime = performance.now() - inferStart;

          timings.push({
            feedPreparationMs: feedTime,
            inferenceMs: inferTime,
            totalMs: feedTime + inferTime,
          });
        }

        const prepared = preprocess(text);
        results.timings.push({
          textLength: text.length,
          sequenceLength: prepared.sequenceLength,
          wordCount: prepared.words.length,
          runs: timings,
          avgInferenceMs: timings.reduce((a, t) => a + t.inferenceMs, 0) / timings.length,
          avgTotalMs: timings.reduce((a, t) => a + t.totalMs, 0) / timings.length,
        });
      }

      await session.release();
    } catch (e) {
      results.errors.push(e.message);
    }

    return results;
  }, { testName, config });

  return result;
}

async function main() {
  const modelDir = join(ARTIFACTS_DIR, MODEL.id.replace("/", "__"), MODEL.variant);

  const sample1kb = `Please contact John Smith regarding the quarterly report. He lives at 742 Evergreen Terrace, Springfield, IL 62704. His email is john.smith@example.com and phone is (555) 123-4567. SSN: 123-45-6789. The meeting with María García-López was rescheduled. Contact her at maria@company.org. Her office is at 456 Oak Avenue, Suite 200, Boston, MA 02108. Visit https://example.com/profile for more info. User ID: user_12345.`;

  const results = [];

  const testConfigs = [
    {
      name: "WASM single-thread (no COI)",
      enableCOI: false,
      config: {
        onnxModel: MODEL.onnxModel,
        dataFile: MODEL.dataFile,
        labels: MODEL.labels,
        testText: sample1kb,
        executionProviders: ["wasm"],
        numThreads: 1,
      },
    },
    {
      name: "WASM multi-thread (with COI)",
      enableCOI: true,
      config: {
        onnxModel: MODEL.onnxModel,
        dataFile: MODEL.dataFile,
        labels: MODEL.labels,
        testText: sample1kb,
        executionProviders: ["wasm"],
        numThreads: 4,
      },
    },
  ];

  for (const test of testConfigs) {
    console.log(`\n${"=".repeat(60)}`);
    console.log(`Running: ${test.name}`);
    console.log("=".repeat(60));

    const server = createTestServer(modelDir, test.enableCOI);
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const port = server.address().port;
    const baseUrl = `http://127.0.0.1:${port}`;

    console.log(`  Server at ${baseUrl} (COI: ${test.enableCOI})`);

    const browser = await chromium.launch({
      headless: true,
      args: test.enableCOI
        ? ["--enable-features=SharedArrayBuffer"]
        : [],
    });

    const context = await browser.newContext();
    const page = await context.newPage();

    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.text().includes("[INFO]")) {
        console.log(`  [browser] ${msg.text()}`);
      }
    });

    const testHtml = `
<!DOCTYPE html>
<html>
<head><title>Latency Test</title></head>
<body>
  <div id="status">Loading...</div>
  <script type="module">
    window.testReady = true;
    document.getElementById('status').textContent = 'Ready';
  </script>
</body>
</html>`;
    writeFileSync(join(ROOT, "public", "latency-test.html"), testHtml);

    await page.goto(`${baseUrl}/latency-test.html`);
    await page.waitForFunction(() => window.testReady === true, { timeout: 30000 });

    const result = await runLatencyTest(page, test.name, test.config);
    results.push(result);

    console.log(`\n  Capabilities:`);
    console.log(`    SharedArrayBuffer: ${result.capabilities.sharedArrayBuffer}`);
    console.log(`    SIMD: ${result.capabilities.simd}`);
    console.log(`    WebGPU: ${result.capabilities.webgpu}`);
    console.log(`    Hardware concurrency: ${result.capabilities.hardwareConcurrency}`);
    console.log(`  Load time: ${result.loadTimeMs?.toFixed(0) || "N/A"} ms`);

    if (result.timings.length > 0) {
      console.log(`\n  Latency by sequence length:`);
      for (const t of result.timings) {
        console.log(
          `    ${t.textLength} chars / ${t.sequenceLength} tokens / ${t.wordCount} words: ${t.avgInferenceMs.toFixed(0)} ms inference, ${t.avgTotalMs.toFixed(0)} ms total`
        );
      }
    }

    if (result.errors.length > 0) {
      console.log(`\n  Errors: ${result.errors.join(", ")}`);
    }

    await browser.close();
    server.close();
  }

  const outPath = join(ROOT, "results", "latency-investigation.json");
  writeFileSync(outPath, JSON.stringify(results, null, 2));
  console.log(`\n\nResults written to ${outPath}`);

  console.log("\n" + "=".repeat(60));
  console.log("SUMMARY");
  console.log("=".repeat(60));

  for (const r of results) {
    console.log(`\n${r.testName}:`);
    console.log(`  Load: ${r.loadTimeMs?.toFixed(0) || "N/A"} ms`);
    if (r.timings.length > 0) {
      const lastTiming = r.timings[r.timings.length - 1];
      console.log(
        `  1KB inference: ${lastTiming?.avgInferenceMs?.toFixed(0) || "N/A"} ms`
      );
    }
    console.log(
      `  SharedArrayBuffer: ${r.capabilities.sharedArrayBuffer}, SIMD: ${r.capabilities.simd}`
    );
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
