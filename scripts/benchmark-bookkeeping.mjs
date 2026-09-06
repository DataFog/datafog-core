import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";
import path from "node:path";
import { performance } from "node:perf_hooks";
import { pathToFileURL } from "node:url";

// Pass separately built baseline/candidate Node package entry points. Both
// native modules remain loaded so measurements can alternate in one process.
const [baselinePath, candidatePath, outputPath] = process.argv.slice(2);
if (!baselinePath || !candidatePath) {
  throw new Error("Usage: node scripts/benchmark-bookkeeping.mjs BASELINE/index.js CANDIDATE/index.js [report.json]");
}
const baseline = await import(pathToFileURL(path.resolve(baselinePath)).href);
const candidate = await import(pathToFileURL(path.resolve(candidatePath)).href);
const policy = { default: { strategy: "redact" }, overrides: { EMAIL: { strategy: "mask" } } };
const customer = () => ({ first_name: "May", contact: "may@example.test", phone: "(212) 555-0100", note: "Order is ready." });
const workloads = [
  { name: "one_customer", data: customer(), iterations: 1000 },
  { name: "100_customers", data: Array.from({ length: 100 }, customer), iterations: 30 },
  { name: "sparse_unicode", data: { note: "👋 plain text. ".repeat(5000) + "may@example.test" }, iterations: 30 },
  ...[128, 512, 1024].map(count => ({ name: `dense_unicode_${count}`, data: { note: "👋 may@example.test ".repeat(count) }, iterations: count === 128 ? 10 : 2 })),
  { name: "dense_ascii_1024", data: { note: "may@example.test ".repeat(1024) }, iterations: 2 },
];
const median = values => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
const results = [];
let sink = 0;
for (const { name, data, iterations } of workloads) {
  const findings = baseline.scanStructured(data).findings;
  const operations = {
    scan: api => api.scanStructured(data),
    transform: api => api.transformStructured(data, findings, policy),
    combined: api => api.scanAndTransformStructured(data, { transform: policy }),
  };
  for (const [operation, run] of Object.entries(operations)) {
    assert.deepEqual(run(candidate), run(baseline), `${name}/${operation}: full result parity`);
    const timings = { baseline: [], candidate: [] };
    const implementations = { baseline, candidate };
    for (const api of Object.values(implementations)) {
      for (let iteration = 0; iteration < Math.min(iterations, 10); iteration++) {
        const result = run(api);
        sink += result.findings?.length ?? result.transformations.length;
      }
    }
    for (let round = 0; round < 7; round++) {
      for (const label of round % 2 ? ["candidate", "baseline"] : ["baseline", "candidate"]) {
        const started = performance.now();
        for (let iteration = 0; iteration < iterations; iteration++) {
          const result = run(implementations[label]);
          sink += result.findings?.length ?? result.transformations.length;
        }
        timings[label].push((performance.now() - started) * 1000 / iterations);
      }
    }
    const before = median(timings.baseline);
    const after = median(timings.candidate);
    const row = { workload: name, operation, bytes: Buffer.byteLength(JSON.stringify(data)), findings: findings.length, iterations, baseline_us: before, candidate_us: after, speedup: before / after, samples_us: timings };
    results.push(row);
    console.log(`${name}/${operation}: ${before.toFixed(3)} -> ${after.toFixed(3)} µs (${row.speedup.toFixed(2)}x)`);
  }
}
if (outputPath) {
  writeFileSync(outputPath, JSON.stringify({ runtime: process.version, platform: process.platform, arch: process.arch, rounds: 7, baselinePath, candidatePath, sink, results }, null, 2) + "\n");
}
