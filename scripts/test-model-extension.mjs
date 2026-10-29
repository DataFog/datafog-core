import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {mkdir, readFile, writeFile} from "node:fs/promises";
import {createRequire} from "node:module";
import os from "node:os";
import path from "node:path";
import {fileURLToPath} from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const harness = path.join(root, "examples/workspace-extension");
const {chromium} = createRequire(path.join(harness, "package.json"))("playwright");
execFileSync("npm", ["run", "build:model", "--prefix", harness], {stdio: "inherit"});
const artifacts = JSON.parse(await readFile(path.join(harness, "dist/model-artifacts.json"), "utf8"));
const text = "👋 John Smith lives at 123 Main Street, Boston, MA 02110. Email john.smith@example.com.";
const password = "synthetic-model-proof-only";
const report = {
  artifacts, syntheticOnly: true, passed: false, tests: [],
  hardware: {platform: process.platform, arch: process.arch, cpu: os.cpus()[0]?.model, logicalCpus: os.cpus().length, totalMemoryBytes: os.totalmem()},
  browser: {}, observations: {externalRequests: [], pageErrors: []}, memory: {},
  measurementNotes: [
    "Cold means first model open in a fresh isolated browser profile using packaged local files; OS filesystem caching is uncontrolled.",
    "Warm means close/reopen in the same extension page and already initialized ONNX Runtime.",
    "Timer gap measures extension-page event-loop responsiveness; this is a fixture observation, not a performance SLA.",
    "JSHeapUsedSize excludes most WASM/ONNX allocations. Process RSS is a sampled sum for the isolated Chromium process set and can double-count shared pages.",
  ],
};
let context, page, pageCDP, browserCDP, sampler;
let peakRSS = 0, samples = 0, sampling = false;
function passed(name) { report.tests.push(name); console.log(`PASS ${name}`); }
async function rss() {
  if (!browserCDP || !["darwin", "linux"].includes(process.platform)) return null;
  try {
    const {processInfo} = await browserCDP.send("SystemInfo.getProcessInfo");
    const pids = processInfo.map(item => item.id).filter(id => Number.isSafeInteger(id) && id > 0);
    const output = execFileSync("ps", ["-o", "rss=", "-p", pids.join(",")], {encoding: "utf8"});
    return output.trim().split(/\s+/).reduce((sum, value) => sum + Number(value) * 1024, 0);
  } catch (error) {
    report.memory.measurementFailures ??= [];
    if (report.memory.measurementFailures.length < 3) report.memory.measurementFailures.push(error.message);
    return null;
  }
}
async function memory(label) {
  const {metrics} = await pageCDP.send("Performance.getMetrics");
  report.memory[label] = {jsHeapUsedBytes: metrics.find(item => item.name === "JSHeapUsedSize")?.value ?? null, processRSSSumBytes: await rss()};
}
async function stop() {
  clearInterval(sampler); sampler = undefined;
  await context?.close(); context = undefined;
}
async function launch(name) {
  await stop();
  const profile = path.join(artifacts.output, "profiles", name);
  await mkdir(profile, {recursive: true});
  context = await chromium.launchPersistentContext(profile, {
    channel: "chromium", headless: process.env.DATAFOG_HEADED !== "1",
    args: ["--enable-automation", `--disable-extensions-except=${artifacts.dist}`, `--load-extension=${artifacts.dist}`],
  });
  report.browser.version = context.browser().version();
  report.browser.channel = "Playwright bundled Chromium";
  context.on("request", request => { if (/^https?:/i.test(request.url())) report.observations.externalRequests.push(request.url()); });
  context.on("page", opened => opened.on("pageerror", error => report.observations.pageErrors.push(error.message)));
  let worker = context.serviceWorkers()[0];
  if (!worker) worker = await context.waitForEvent("serviceworker", {timeout: 30000});
  // WHATWG URL.origin is "null" for chrome-extension URLs; use the actual scheme and host.
  const extensionOrigin = `chrome-extension://${new URL(worker.url()).hostname}`;
  report.browser.extensionOrigin = extensionOrigin;
  page = await context.newPage();
  await page.goto(`${extensionOrigin}/model-page.html`);
  await page.waitForFunction(() => Boolean(window.modelProof), {timeout: 30000});
  assert.equal(new URL(page.url()).protocol, "chrome-extension:");
  const manifest = await page.evaluate(() => chrome.runtime.getManifest());
  assert.equal(manifest.manifest_version, 3);
  assert.match(manifest.content_security_policy.extension_pages, /wasm-unsafe-eval/);
  assert.doesNotMatch(manifest.content_security_policy.extension_pages, /(?:https?:|blob:|'unsafe-eval')/);
  browserCDP = await context.browser().newBrowserCDPSession();
  pageCDP = await context.newCDPSession(page);
  await pageCDP.send("Performance.enable");
  sampler = setInterval(async () => {
    if (sampling) return;
    sampling = true;
    try { const value = await rss(); if (value !== null) { peakRSS = Math.max(peakRSS, value); samples++; } }
    finally { sampling = false; }
  }, 200);
}
async function call(name, ...args) { return page.evaluate(([name, args]) => window.modelProof[name](...args), [name, args]); }
try {
  await launch("positive");
  await memory("beforeModel");
  report.coldLoad = await call("openModel");
  await memory("afterColdLoad");
  passed("candidate npm tarball and bundled local model load under MV3 CSP");
  const first = await call("scan", text);
  report.firstInference = first;
  assert(first.value.findings.some(finding => finding.entityType === "PERSON" && finding.matchedText === "John Smith"));
  assert(first.value.findings.some(finding => finding.entityType === "EMAIL" && finding.matchedText === "john.smith@example.com"));
  for (const finding of first.value.findings) {
    assert.equal(Buffer.from(text).subarray(finding.byteRange.start, finding.byteRange.end).toString(), finding.matchedText);
    assert.equal([...text].slice(finding.codepointRange.start, finding.codepointRange.end).join(""), finding.matchedText);
    assert.equal(text.slice(finding.utf16Range.start, finding.utf16Range.end), finding.matchedText);
  }
  report.warmInference = [];
  for (let index = 0; index < 3; index++) report.warmInference.push(await call("scan", text));
  passed("actual GLiNER2.5 Small findings and all offset units match synthetic source");
  const concurrency = await page.evaluate(async text => {
    const pending = window.modelProof.scan(text);
    let scan, close;
    try { await window.modelProof.scan(text); } catch (error) { scan = error.code; }
    try { await window.modelProof.closeModel(); } catch (error) { close = error.code; }
    await pending; return {scan, close};
  }, text);
  assert.deepEqual(concurrency, {scan: "model_session_busy", close: "model_session_busy"});
  passed("single-flight inference and close-during-inference fail explicitly");
  await call("openWorkspace", password);
  assert.equal(await call("seedStale", text, first.value.findings), 31);
  const prepared = await call("prepare", text);
  assert(!prepared.preview.text.includes("John Smith"));
  assert.equal(await page.locator("#sink").textContent(), "");
  const uncommitted = await call("restoreReport", prepared.preview.text);
  assert.equal(uncommitted.text, null);
  assert(uncommitted.issues.length > 0);
  const failedSave = await page.evaluate(async () => {
    try { await window.modelProof.approve(true); return "unexpected_success"; }
    catch (error) { return error.code; }
  });
  assert.equal(failedSave, "storage_failure");
  assert.equal(await page.locator("#sink").textContent(), "");
  const approved = await call("approve");
  assert.equal(approved.restored, text);
  assert.equal(await page.locator("#sink").textContent(), approved.text);
  const stored = JSON.stringify(await call("records"));
  assert(!stored.includes("John Smith") && !stored.includes("john.smith@example.com"));
  report.planReclamation = await call("planReclamation");
  const damaged = await call("restoreReport", `${approved.text} [[PERSON_BAD]]`);
  assert.equal(damaged.text, null);
  assert(damaged.issues.length > 0);
  passed("model findings prepare encrypted durable save before activation; failed save releases nothing; strict restore is atomic");
  await memory("afterInferenceAndWorkspace");
  const descriptor = await call("descriptor");
  await call("closeModel"); await call("closeModel");
  assert.deepEqual(await call("descriptor"), descriptor);
  const closed = await page.evaluate(async text => {
    try { await window.modelProof.scan(text); return "unexpected_success"; }
    catch (error) { return error.code; }
  }, text);
  assert.equal(closed, "model_session_closed");
  report.warmLoad = await call("openModel");
  assert((await call("scan", text)).value.findings.length > 0);
  passed("idempotent close, closed-session rejection and explicit warm reopen");
  // Terminate the browser with the reopened model still live, as when its owner tab disappears.
  await launch("positive");
  assert.equal(await call("hasModel"), false);
  await call("openWorkspace", password);
  assert.equal(await call("restore", approved.text), text);
  report.sourcePreflight = await call("sourcePreflight");
  await call("closeWorkspace");
  passed("browser process restart loses model session and rehydrates encrypted mappings");
  for (const variant of ["bad-model", "missing-wasm"]) {
    await launch(variant);
    const error = await page.evaluate(async variant => {
      try { await window.modelProof.openModel(variant); return {code: "unexpected_success"}; }
      catch (error) { return {code: error.code, message: error.message}; }
    }, variant);
    assert.equal(error.code, "model_load_failed");
    assert.equal(error.message, "browser model operation failed");
    passed(`${variant} returns sanitized load failure under extension CSP`);
  }
  assert.deepEqual(report.observations.externalRequests, []);
  assert.deepEqual(report.observations.pageErrors, []);
  assert.equal(report.sourcePreflight.blocked, true, "candidate is missing damaged-source preflight (#128)");
  assert.equal(report.sourcePreflight.code, "invalid_marker");
  assert.equal(report.sourcePreflight.markerIssues.length, 1);
  const issue = report.sourcePreflight.markerIssues[0];
  assert.deepEqual(Object.keys(issue).sort(), ["byteRange", "codepointRange", "kind", "utf16Range"]);
  assert.equal(issue.kind, "damaged");
  const source = "👋 e\u0301 literal [[my_note]]";
  assert.equal(Buffer.from(source).subarray(issue.byteRange.start, issue.byteRange.end).toString(), "[[my_note]]");
  assert.equal([...source].slice(issue.codepointRange.start, issue.codepointRange.end).join(""), "[[my_note]]");
  assert.equal(source.slice(issue.utf16Range.start, issue.utf16Range.end), "[[my_note]]");
  assert(!JSON.stringify(report.sourcePreflight).includes("my_note"));
  assert.equal(report.planReclamation.accepted, true, "candidate retains stale plans after activation (#129)");
  passed("candidate includes source-marker preflight and stale-plan reclamation");
  report.passed = true;
} catch (error) {
  report.failure = {message: error.message, stack: error.stack};
  process.exitCode = 1;
  console.error(error);
} finally {
  report.memory.peakProcessRSSSumBytes = peakRSS || null;
  report.memory.processRSSSamples = samples;
  await stop();
  await writeFile(path.join(artifacts.output, "report.json"), JSON.stringify(report, null, 2));
  console.log(`MV3 proof report: ${path.join(artifacts.output, "report.json")}`);
}
