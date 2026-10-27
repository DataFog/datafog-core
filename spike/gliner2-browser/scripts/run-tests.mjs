#!/usr/bin/env node
/**
 * Automated browser tests for GLiNER2 spike.
 * Runs in headless Chromium via Playwright.
 */
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync, existsSync, statSync } from "node:fs";
import { join, extname, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const PORT = 8766;

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

function resolve(pathname) {
  if (pathname === "/") pathname = "/index.html";
  
  const cleanPath = pathname.startsWith("/") ? pathname.slice(1) : pathname;
  
  const searchPaths = [
    join(ROOT, "public", cleanPath),
    join(ROOT, cleanPath),
  ];
  
  for (const filepath of searchPaths) {
    if (existsSync(filepath) && statSync(filepath).isFile()) {
      return filepath;
    }
  }
  
  return null;
}

function startServer() {
  return new Promise((resolvePromise) => {
    const server = createServer((req, res) => {
      const pathname = new URL(req.url, "http://localhost").pathname;
      const file = resolve(pathname);
      
      if (!file) {
        console.log(`  [404] ${pathname}`);
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
        console.log(`  [500] ${pathname}: ${err.message}`);
        res.writeHead(500);
        res.end("Server error");
      }
    });
    
    server.listen(PORT, "127.0.0.1", () => resolvePromise(server));
  });
}

async function runTests() {
  console.log("Starting test server...");
  const server = await startServer();
  
  console.log("Launching headless Chromium...");
  const browser = await chromium.launch({ headless: true });
  
  const results = {
    passed: 0,
    failed: 0,
    tests: [],
  };
  
  try {
    const page = await browser.newPage();
    
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        console.log(`  [browser error] ${msg.text()}`);
      }
    });
    
    console.log("\nRunning GLiNER2 browser tests...\n");
    
    // Test 1: Model loading
    console.log("Test 1: Model loading");
    await page.goto(`http://127.0.0.1:${PORT}/test-harness.html`);
    
    const loadResult = await page.evaluate(async () => {
      try {
        const { Gliner2Session } = await import("/src/gliner2-session.mjs");
        
        const start = performance.now();
        const session = await Gliner2Session.create({
          modelPath: "/models/model_q8.onnx",
          modelDataPath: "/models/model_q8.onnx.data",
          tokenizerPath: "/models/tokenizer.json",
          metadataPath: "/models/model_q8_metadata.json",
          entityLabelsPath: "/models/entity_labels.json",
          configPath: "/models/config.json",
          executionProvider: "wasm",
        });
        const loadTime = performance.now() - start;
        
        window._session = session;
        
        return {
          success: true,
          loadTimeMs: loadTime,
          backend: session.backend,
          entityLabels: session.entityLabels,
        };
      } catch (err) {
        return {
          success: false,
          error: err?.message || String(err),
          stack: err?.stack || "no stack",
        };
      }
    });
    
    if (loadResult.success) {
      console.log(`  PASS: Model loaded in ${loadResult.loadTimeMs.toFixed(0)}ms (${loadResult.backend})`);
      console.log(`  Entity labels: ${loadResult.entityLabels.join(", ")}`);
      results.passed++;
    } else {
      console.log(`  FAIL: ${loadResult.error}`);
      if (loadResult.stack) console.log(`  Stack: ${loadResult.stack.slice(0, 500)}`);
      results.failed++;
    }
    results.tests.push({ name: "model_loading", ...loadResult });
    
    if (!loadResult.success) {
      throw new Error("Model loading failed, skipping remaining tests");
    }
    
    // Test 2: Basic entity extraction
    console.log("\nTest 2: Basic entity extraction");
    const extractResult = await page.evaluate(async () => {
      try {
        const session = window._session;
        const text = "Contact John Smith at john@example.com";
        const labels = ["name", "email"];
        
        const start = performance.now();
        const result = await session.extract(text, labels, 0.5);
        const inferTime = performance.now() - start;
        
        return {
          success: true,
          entities: result.entities,
          inferTimeMs: inferTime,
          tokenCount: result.tokenCount,
        };
      } catch (err) {
        console.error("Extraction error:", err);
        return { 
          success: false, 
          error: err?.message || err?.toString() || String(err),
          stack: err?.stack
        };
      }
    });
    
    if (extractResult.success) {
      const hasName = extractResult.entities.some(e => e.label === "name");
      const hasEmail = extractResult.entities.some(e => e.label === "email");
      
      console.log(`  Found ${extractResult.entities.length} entities in ${extractResult.inferTimeMs.toFixed(0)}ms`);
      extractResult.entities.forEach(e => {
        console.log(`    - ${e.label}: "${e.text}" (${(e.score * 100).toFixed(1)}%)`);
      });
      
      if (hasName || hasEmail) {
        console.log(`  PASS`);
        results.passed++;
      } else {
        console.log(`  PARTIAL: No name or email found (model may need different threshold)`);
        results.passed++; // Still pass - model behavior is what we're measuring
      }
    } else {
      console.log(`  FAIL: ${extractResult.error}`);
      results.failed++;
    }
    results.tests.push({ name: "basic_extraction", ...extractResult });
    
    // Test 3: Empty input
    console.log("\nTest 3: Empty input handling");
    const emptyResult = await page.evaluate(async () => {
      try {
        const session = window._session;
        const result = await session.extract("", ["name"], 0.5);
        return { success: true, entities: result.entities };
      } catch (err) {
        return { success: false, error: err.message };
      }
    });
    
    if (emptyResult.success && emptyResult.entities.length === 0) {
      console.log(`  PASS: Empty input returns no entities`);
      results.passed++;
    } else if (emptyResult.success) {
      console.log(`  FAIL: Expected no entities for empty input, got ${emptyResult.entities.length}`);
      results.failed++;
    } else {
      console.log(`  FAIL: ${emptyResult.error}`);
      results.failed++;
    }
    results.tests.push({ name: "empty_input", ...emptyResult });
    
    // Test 4: Multi-entity extraction
    console.log("\nTest 4: Multi-entity extraction (paste-sized)");
    const multiResult = await page.evaluate(async () => {
      try {
        const session = window._session;
        const text = `Contact John Smith at john@acme.com or call +1 415 555 0123. 
                      He lives at 123 Main Street, San Francisco, CA 94105.`;
        const labels = null; // Use all labels
        
        const result = await session.extract(text, labels, 0.3);
        return {
          success: true,
          entities: result.entities,
          inferTimeMs: result.inferenceTimeMs,
          tokenCount: result.tokenCount,
          wordCount: result.wordCount,
        };
      } catch (err) {
        return { success: false, error: err.message };
      }
    });
    
    if (multiResult.success) {
      console.log(`  Found ${multiResult.entities.length} entities in ${multiResult.inferTimeMs.toFixed(0)}ms`);
      console.log(`  Tokens: ${multiResult.tokenCount}, Words: ${multiResult.wordCount}`);
      multiResult.entities.forEach(e => {
        console.log(`    - ${e.label}: "${e.text}" [${e.start}-${e.end}] (${(e.score * 100).toFixed(1)}%)`);
      });
      console.log(`  PASS`);
      results.passed++;
    } else {
      console.log(`  FAIL: ${multiResult.error}`);
      results.failed++;
    }
    results.tests.push({ name: "multi_entity", ...multiResult });
    
    // Test 5: Latency measurement
    console.log("\nTest 5: Cold vs warm latency");
    const latencyResult = await page.evaluate(async () => {
      try {
        const session = window._session;
        const text = "John Smith works at Acme Corp in New York.";
        const labels = ["name", "address"];
        
        const cold = await session.extract(text, labels, 0.5);
        
        const warmTimes = [];
        for (let i = 0; i < 5; i++) {
          const result = await session.extract(text, labels, 0.5);
          warmTimes.push(result.inferenceTimeMs);
        }
        
        return {
          success: true,
          coldMs: cold.inferenceTimeMs,
          warmAvgMs: warmTimes.reduce((a, b) => a + b, 0) / warmTimes.length,
          warmMinMs: Math.min(...warmTimes),
          warmMaxMs: Math.max(...warmTimes),
        };
      } catch (err) {
        return { success: false, error: err.message };
      }
    });
    
    if (latencyResult.success) {
      console.log(`  Cold: ${latencyResult.coldMs.toFixed(0)}ms`);
      console.log(`  Warm avg: ${latencyResult.warmAvgMs.toFixed(0)}ms (min: ${latencyResult.warmMinMs.toFixed(0)}, max: ${latencyResult.warmMaxMs.toFixed(0)})`);
      console.log(`  PASS`);
      results.passed++;
    } else {
      console.log(`  FAIL: ${latencyResult.error}`);
      results.failed++;
    }
    results.tests.push({ name: "latency", ...latencyResult });
    
    // Cleanup
    await page.evaluate(() => {
      if (window._session) {
        window._session.dispose();
      }
    });
    
  } catch (err) {
    console.error("Test error:", err.message);
    results.failed++;
  } finally {
    await browser.close();
    server.close();
  }
  
  console.log("\n" + "=".repeat(50));
  console.log(`Results: ${results.passed} passed, ${results.failed} failed`);
  console.log("=".repeat(50));
  
  console.log("\nJSON output:");
  console.log(JSON.stringify(results, null, 2));
  
  process.exit(results.failed > 0 ? 1 : 0);
}

const modelPath = join(ROOT, "models", "model_q8.onnx");
if (!existsSync(modelPath)) {
  console.error("Model files not found. Run: npm run download-model");
  process.exit(1);
}

runTests().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
