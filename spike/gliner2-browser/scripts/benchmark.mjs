#!/usr/bin/env node
/**
 * Benchmark GLiNER2 browser inference latency.
 * 
 * Measures cold start (model loading) and warm inference times
 * for various input sizes.
 */
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync, existsSync, statSync, writeFileSync } from "node:fs";
import { join, extname, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const PORT = 8768;

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

// Benchmark inputs of various sizes
const BENCHMARK_INPUTS = [
  {
    name: "tiny",
    text: "Contact John at john@example.com.",
    targetBytes: 40,
  },
  {
    name: "small",
    text: "Dear Support, my name is John Smith. Email: john.smith@acme.com, Phone: +1 415 555 0123.",
    targetBytes: 100,
  },
  {
    name: "medium",
    text: `Dear Customer Support Team,

My name is Sarah Johnson and I am writing regarding my recent order (#ORD-2024-78432).

I can be reached at:
- Email: sarah.johnson@techcorp.io
- Phone: +44 20 7946 0958
- Mobile: +44 7700 900123

My shipping address is:
45 Baker Street, Apartment 3B
London, W1U 8EW
United Kingdom

Please let me know if you need any additional information.

Best regards,
Sarah Johnson
Customer ID: CUS-2024-12345`,
    targetBytes: 500,
  },
  {
    name: "paste_1kb",
    text: `MEETING NOTES - Q4 Planning Session

Date: October 15, 2024
Location: Conference Room A, 500 Technology Drive, Suite 400, Palo Alto, CA 94301

Attendees:
- Michael Chen (michael.chen@techcorp.io, ext. 4521)
- Emily Rodriguez (emily.r@techcorp.io, +1 650 555 0147)
- David Kim (d.kim@techcorp.io)
- Lisa Wang (lisa.wang@consultant.biz, +1 415 555 0198)
- James Wilson (j.wilson@techcorp.io, +1 408 555 0156)

Action Items:
1. Michael to review budget proposal by Oct 20 - contact finance@techcorp.io
2. Emily to schedule follow-up with vendor at 1234 Innovation Blvd, San Jose, CA 95134
3. David to update CRM records for accounts #A-78432 and #B-91256
4. Lisa to prepare presentation for client meeting on Oct 25

Financial Summary:
- Q4 revenue target: $2.5M
- Marketing budget: $150,000
- Engineering headcount: 12 FTE

Office Relocation:
- New address starting Nov 1: 789 Enterprise Way, Mountain View, CA 94043
- Contact facilities@techcorp.io for access badges

IT Support:
- Help desk: help@techcorp.io or +1 650 555 0199
- Security: security@techcorp.io (24/7)

Next meeting: October 22, 2024 at 2:00 PM PST
Video: https://meet.techcorp.io/q4-planning`,
    targetBytes: 1000,
  },
  {
    name: "paste_2kb",
    text: `QUARTERLY BUSINESS REVIEW - TechCorp Industries Q3 2024

Prepared by: Strategic Planning Division
Date: October 1, 2024

EXECUTIVE SUMMARY

This report covers Q3 2024 performance for TechCorp Industries. For questions, contact:
- CEO: Robert Thompson (robert.thompson@techcorp.io, +1 650 555 0100)
- CFO: Jennifer Martinez (jennifer.m@techcorp.io, +1 650 555 0101)
- COO: Michael Chen (michael.chen@techcorp.io, +1 650 555 0102)

REGIONAL PERFORMANCE

North America (HQ: 500 Technology Drive, Palo Alto, CA 94301)
- Regional Director: Sarah Johnson (sarah.johnson@techcorp.io)
- Q3 Revenue: $4.2M
- Key accounts: Acme Corp (#A-10001), GlobalTech (#G-20002), InnovateCo (#I-30003)

Europe (Office: 123 Innovation Street, London, EC2A 4BQ, UK)
- Regional Director: Hans Mueller (hans.mueller@techcorp.eu)
- Phone: +44 20 7946 0958
- Q3 Revenue: €2.8M
- Key accounts: EuroBank (#E-40001), TechGmbH (#T-50002)

Asia Pacific (Office: 456 Business Park, Singapore 018936)
- Regional Director: Wei Zhang (wei.zhang@techcorp.sg)
- Phone: +65 6789 0123
- Q3 Revenue: $3.1M
- Key accounts: AsiaFirst (#A-60001), PacificTech (#P-70002)

EMPLOYEE DIRECTORY UPDATES

New Hires Q3:
- David Kim (d.kim@techcorp.io) - Senior Engineer, Employee ID: EMP-2024-789
- Lisa Wang (lisa.wang@techcorp.io) - Product Manager, Employee ID: EMP-2024-790
- James Wilson (j.wilson@techcorp.io) - Sales Director, Employee ID: EMP-2024-791

Departures:
- Emily Rodriguez (former: emily.r@techcorp.io) - Last day: Sept 30, 2024

OFFICE LOCATIONS

Headquarters:
500 Technology Drive, Suite 400
Palo Alto, CA 94301
Main: +1 650 555 0000
Fax: +1 650 555 0001

R&D Center:
789 Innovation Blvd
San Jose, CA 95134
Lab: +1 408 555 0200

Customer Support:
1234 Service Road
Austin, TX 78701
Support: +1 512 555 0300
Email: support@techcorp.io

FINANCIAL CONTACTS

Accounts Payable: ap@techcorp.io
Accounts Receivable: ar@techcorp.io
Tax Department: tax@techcorp.io
Wire Transfer Info: Bank of America, Routing: 026009593, Account: 12345678901

HR CONTACTS

Benefits: benefits@techcorp.io
Payroll: payroll@techcorp.io
Recruiting: careers@techcorp.io

For emergencies, contact Security at +1 650 555 0911 or security@techcorp.io.`,
    targetBytes: 2000,
  },
];

const DEFAULT_LABELS = ["name", "email", "phone_num", "address", "id_num"];

async function runBenchmark() {
  console.log("GLiNER2 Browser Benchmark");
  console.log("=========================\n");
  
  console.log("Starting test server...");
  const server = await startServer();
  
  console.log("Launching headless Chromium...");
  const browser = await chromium.launch({ headless: true });
  
  const results = {
    timestamp: new Date().toISOString(),
    environment: {
      nodeVersion: process.version,
      platform: process.platform,
      arch: process.arch,
    },
    coldStart: null,
    warmInference: [],
  };
  
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${PORT}/test-harness.html`);
    
    // Measure cold start (full session creation)
    console.log("Measuring cold start...");
    const coldResult = await page.evaluate(async () => {
      const { Gliner2Session } = await import("/src/gliner2-session.mjs");
      
      const start = performance.now();
      const session = await Gliner2Session.create({
        modelPath: "/models/model_q8.onnx",
        tokenizerPath: "/models/tokenizer.json",
        metadataPath: "/models/model_q8_metadata.json",
        configPath: "/models/config.json",
        executionProvider: "wasm",
      });
      const loadTime = performance.now() - start;
      
      window._session = session;
      
      return {
        loadTimeMs: loadTime,
        backend: session.backend,
      };
    });
    
    results.coldStart = coldResult;
    console.log(`  Cold start: ${coldResult.loadTimeMs.toFixed(0)}ms (${coldResult.backend})\n`);
    
    // Warm up
    console.log("Warming up...");
    await page.evaluate(async () => {
      const session = window._session;
      await session.extract("Warmup text with john@example.com", ["email"], 0.5);
      await session.extract("More warmup text", ["name"], 0.5);
    });
    
    // Benchmark each input size
    console.log("\nBenchmarking inference times:\n");
    
    for (const input of BENCHMARK_INPUTS) {
      const actualBytes = new TextEncoder().encode(input.text).length;
      console.log(`${input.name} (${actualBytes} bytes, ~${input.text.split(/\s+/).length} words):`);
      
      const benchResult = await page.evaluate(async ({ text, labels, iterations }) => {
        const session = window._session;
        const times = [];
        let entities = [];
        
        for (let i = 0; i < iterations; i++) {
          const result = await session.extract(text, labels, 0.5);
          times.push(result.inferenceTimeMs);
          if (i === 0) {
            entities = result.entities;
          }
        }
        
        times.sort((a, b) => a - b);
        
        return {
          times,
          p50: times[Math.floor(times.length * 0.5)],
          p95: times[Math.floor(times.length * 0.95)],
          min: times[0],
          max: times[times.length - 1],
          avg: times.reduce((a, b) => a + b, 0) / times.length,
          entityCount: entities.length,
        };
      }, { text: input.text, labels: DEFAULT_LABELS, iterations: 10 });
      
      console.log(`  p50: ${benchResult.p50.toFixed(0)}ms, p95: ${benchResult.p95.toFixed(0)}ms`);
      console.log(`  min: ${benchResult.min.toFixed(0)}ms, max: ${benchResult.max.toFixed(0)}ms, avg: ${benchResult.avg.toFixed(0)}ms`);
      console.log(`  entities found: ${benchResult.entityCount}`);
      console.log();
      
      results.warmInference.push({
        name: input.name,
        bytes: actualBytes,
        words: input.text.split(/\s+/).length,
        ...benchResult,
      });
    }
    
    // Cleanup
    await page.evaluate(() => window._session?.dispose());
    
  } catch (err) {
    console.error("Benchmark error:", err);
  } finally {
    await browser.close();
    server.close();
  }
  
  // Summary
  console.log("=".repeat(50));
  console.log("Summary:");
  console.log(`  Cold start: ${results.coldStart?.loadTimeMs.toFixed(0)}ms`);
  console.log(`  Backend: ${results.coldStart?.backend}`);
  console.log("\n  Warm inference (p50):");
  for (const r of results.warmInference) {
    console.log(`    ${r.name}: ${r.p50.toFixed(0)}ms`);
  }
  console.log("=".repeat(50));
  
  // Write results
  const outputPath = join(ROOT, "benchmark-results.json");
  writeFileSync(outputPath, JSON.stringify(results, null, 2));
  console.log(`\nResults written to: ${outputPath}`);
}

// Check if models exist
const modelPath = join(ROOT, "models", "model_q8.onnx");
if (!existsSync(modelPath)) {
  console.error("Model files not found. Run: npm run download-model");
  process.exit(1);
}

runBenchmark().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
