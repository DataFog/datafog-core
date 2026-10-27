#!/usr/bin/env node
/**
 * Parity check: Compare browser GLiNER2 spans to Rust core spans.
 * 
 * This spike can only test tokenizer alignment and basic span decoding.
 * Full parity with the Rust GLiNER2 worker requires:
 * 1. Same model weights (we use the same okasi/gliner2-privacy-filter-pii-multi-onnx)
 * 2. Same tokenization (we match the Python tokenizer)
 * 3. Same span decoding logic (sigmoid threshold, NMS)
 * 4. Same count-floor fix (issue #87)
 * 
 * Since the Rust core uses a Python subprocess for GLiNER2, and this spike
 * uses ONNX directly, there may be numerical differences in the span scores.
 * This check focuses on offset alignment, not exact score matching.
 */
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync, existsSync, statSync, writeFileSync } from "node:fs";
import { join, extname, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const WORKSPACE = join(ROOT, "..", "..");
const PORT = 8767;

const MIME_TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".wasm": "application/wasm",
  ".onnx": "application/octet-stream",
  ".data": "application/octet-stream",
};

function resolve(url) {
  let path = url.split("?")[0];
  if (path === "/") path = "/index.html";
  
  const publicPath = join(ROOT, "public", path);
  if (existsSync(publicPath) && statSync(publicPath).isFile()) {
    return publicPath;
  }
  
  const rootPath = join(ROOT, path);
  if (existsSync(rootPath) && statSync(rootPath).isFile()) {
    return rootPath;
  }
  
  return null;
}

function startServer() {
  return new Promise((resolve) => {
    const server = createServer((req, res) => {
      const file = resolve(req.url);
      
      if (!file) {
        res.writeHead(404);
        res.end("Not found");
        return;
      }
      
      const ext = extname(file);
      const contentType = MIME_TYPES[ext] || "application/octet-stream";
      
      try {
        const data = readFileSync(file);
        res.writeHead(200, { 
          "Content-Type": contentType,
          "Cross-Origin-Opener-Policy": "same-origin",
          "Cross-Origin-Embedder-Policy": "require-corp",
        });
        res.end(data);
      } catch (err) {
        res.writeHead(500);
        res.end("Server error");
      }
    });
    
    server.listen(PORT, "127.0.0.1", () => resolve(server));
  });
}

// Test fixtures designed to exercise offset alignment
const PARITY_FIXTURES = [
  {
    id: "simple_email",
    text: "Contact john@example.com for info.",
    labels: ["email"],
  },
  {
    id: "simple_name",
    text: "Hello John Smith!",
    labels: ["name"],
  },
  {
    id: "multibyte_emoji",
    text: "👋 Contact John at john@example.com",
    labels: ["name", "email"],
  },
  {
    id: "multibyte_umlaut",
    text: "Müller can be reached at mueller@example.de",
    labels: ["name", "email"],
  },
  {
    id: "multi_entity",
    text: "John Smith (john@acme.com, +1 415 555 0123) lives at 123 Main St.",
    labels: ["name", "email", "phone_num", "address"],
  },
  {
    id: "paste_sized",
    text: `Dear Support Team,

My name is Sarah Johnson and I need help with order #12345. 
You can reach me at sarah.johnson@company.org or call +44 20 7946 0958.
My shipping address is 45 Baker Street, London, W1U 8EW.

Thanks,
Sarah`,
    labels: ["name", "email", "phone_num", "address"],
  },
];

async function runParityCheck() {
  console.log("GLiNER2 Browser Parity Check");
  console.log("============================\n");
  
  console.log("Starting test server...");
  const server = await startServer();
  
  console.log("Launching headless Chromium...");
  const browser = await chromium.launch({ headless: true });
  
  const results = {
    timestamp: new Date().toISOString(),
    fixtures: [],
    summary: {
      total: PARITY_FIXTURES.length,
      withEntities: 0,
      offsetsValid: 0,
    },
  };
  
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${PORT}/test-harness.html`);
    
    // Load session
    console.log("Loading GLiNER2 session...\n");
    await page.evaluate(async () => {
      const { Gliner2Session } = await import("/src/gliner2-session.mjs");
      window._session = await Gliner2Session.create({
        modelPath: "/models/model_q8.onnx",
        tokenizerPath: "/models/tokenizer.json",
        metadataPath: "/models/model_q8_metadata.json",
        configPath: "/models/config.json",
        executionProvider: "wasm",
      });
    });
    
    // Run fixtures
    for (const fixture of PARITY_FIXTURES) {
      console.log(`Fixture: ${fixture.id}`);
      console.log(`  Text: "${fixture.text.slice(0, 60)}${fixture.text.length > 60 ? '...' : ''}"`);
      
      const result = await page.evaluate(async ({ text, labels }) => {
        const session = window._session;
        const output = await session.extract(text, labels, 0.5);
        return output;
      }, { text: fixture.text, labels: fixture.labels });
      
      // Validate offsets
      let offsetsValid = true;
      for (const entity of result.entities) {
        const extracted = fixture.text.slice(entity.start, entity.end);
        if (extracted !== entity.text) {
          console.log(`  [OFFSET MISMATCH] ${entity.label}: expected "${entity.text}", got "${extracted}" at [${entity.start}-${entity.end}]`);
          offsetsValid = false;
        }
      }
      
      if (result.entities.length > 0) {
        results.summary.withEntities++;
        console.log(`  Found ${result.entities.length} entities:`);
        for (const e of result.entities) {
          const charRange = `[${e.start}-${e.end}]`;
          // Compute byte and codepoint ranges for comparison
          const byteStart = new TextEncoder().encode(fixture.text.slice(0, e.start)).length;
          const byteEnd = new TextEncoder().encode(fixture.text.slice(0, e.end)).length;
          const cpStart = [...fixture.text.slice(0, e.start)].length;
          const cpEnd = [...fixture.text.slice(0, e.end)].length;
          
          console.log(`    - ${e.label}: "${e.text}" char${charRange} byte[${byteStart}-${byteEnd}] cp[${cpStart}-${cpEnd}]`);
        }
      } else {
        console.log(`  No entities found`);
      }
      
      if (offsetsValid) {
        results.summary.offsetsValid++;
        console.log(`  [OK] Offsets valid`);
      }
      
      results.fixtures.push({
        ...fixture,
        entities: result.entities,
        inferenceTimeMs: result.inferenceTimeMs,
        offsetsValid,
      });
      
      console.log();
    }
    
    // Cleanup
    await page.evaluate(() => window._session?.dispose());
    
  } catch (err) {
    console.error("Parity check error:", err);
  } finally {
    await browser.close();
    server.close();
  }
  
  // Summary
  console.log("=".repeat(50));
  console.log("Summary:");
  console.log(`  Total fixtures: ${results.summary.total}`);
  console.log(`  With entities: ${results.summary.withEntities}`);
  console.log(`  Offsets valid: ${results.summary.offsetsValid}/${results.summary.total}`);
  console.log("=".repeat(50));
  
  // Write results
  const outputPath = join(ROOT, "parity-results.json");
  writeFileSync(outputPath, JSON.stringify(results, null, 2));
  console.log(`\nResults written to: ${outputPath}`);
  
  // Exit code
  const success = results.summary.offsetsValid === results.summary.total;
  process.exit(success ? 0 : 1);
}

// Check if models exist
const modelPath = join(ROOT, "models", "model_q8.onnx");
if (!existsSync(modelPath)) {
  console.error("Model files not found. Run: npm run download-model");
  process.exit(1);
}

runParityCheck().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
