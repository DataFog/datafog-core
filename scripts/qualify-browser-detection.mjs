import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {createHash} from "node:crypto";
import {mkdir, mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {createRequire} from "node:module";
import os from "node:os";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {scoreCorpus, validateSpan} from "./browser-detection-scoring.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const harness = path.join(root, "examples/workspace-extension");
const requireHarness = createRequire(path.join(harness, "package.json"));
const {chromium} = requireHarness("playwright");
const {build} = requireHarness("esbuild");
const corpusPath = path.join(root, "fixtures/browser-detection/qualification-v1.json");
const corpus = JSON.parse(await readFile(corpusPath));
const output = path.join(root, "target/browser-detection", new Date().toISOString().replaceAll(/[:.]/g, "-"));
await mkdir(output, {recursive: true});
for (const record of corpus.records) for (const gold of [...record.gold, ...record.streetLineGold]) validateSpan(record.text, gold);
execFileSync("npm", ["run", "build:model", "--prefix", harness], {stdio: "inherit"});
const artifacts = JSON.parse(await readFile(path.join(harness, "dist/model-artifacts.json")));
const modules = path.join(artifacts.output, "consumer/node_modules");
// This test page consumes the installed tarball; inference/routing stays in the real SDK.
await build({
  stdin: {contents: `import {init} from "@datafog/wasm";
import {ModelSession} from "@datafog/wasm/models";
await init();
let model;
window.browserQualification = {
  async open(options) {
    model = await ModelSession.open({...options, modelBaseUrl: chrome.runtime.getURL("models/"), wasmPaths: chrome.runtime.getURL("ort/")});
    return model.describe();
  },
  scan: text => model.scan(text),
  close: () => model.close(),
};`, resolveDir: path.join(artifacts.output, "consumer"), sourcefile: "qualification-entry.mjs"},
  outfile: path.join(artifacts.dist, "qualification-entry.js"), bundle: true, format: "esm", platform: "browser", target: "chrome120", nodePaths: [modules], legalComments: "eof",
});
await writeFile(path.join(artifacts.dist, "qualification.html"), '<!doctype html><meta charset="utf-8"><title>Synthetic detection qualification preparation</title><script type="module" src="qualification-entry.js"></script>');
const digest = async file => createHash("sha256").update(await readFile(file)).digest("hex");
const inputs = ["scripts/qualify-browser-detection.mjs", "scripts/browser-detection-scoring.mjs", "scripts/test-browser-detection-scoring.mjs", "fixtures/browser-detection/qualification-v1.json", "bindings/wasm/models/model.mjs", "bindings/wasm/package-lock.json", "examples/workspace-extension/package-lock.json"];
const receipt = {
  kind: "shipping-adapter-preparation-baseline", qualificationComplete: false,
  pendingFinalWorkflow: [141, 144, 145, 146], syntheticOnly: true,
  sourceCommit: artifacts.sourceCommit, sourceDirty: artifacts.sourceDirty,
  inputs: Object.fromEntries(await Promise.all(inputs.map(async file => [file, await digest(path.join(root, file))]))),
  model: artifacts.model, package: {...artifacts.tarball, file: path.basename(artifacts.tarball.file)},
  versions: artifacts.versions, consumerLockSha256: artifacts.consumerLockSha256,
  assets: artifacts.assets, extensionFiles: artifacts.extensionFiles,
  qualificationEntrySha256: await digest(path.join(artifacts.dist, "qualification-entry.js")),
  options: corpus.model, config: {},
  matching: {exact: "same type and both byte boundaries", overlap: "same type and positive byte overlap; maximum one-to-one matching; diagnostic, not full-span coverage"},
  addressContract: "ADDRESS is full-span shipping gold. STREET_ADDRESS is a separate street-line diagnostic using untrimmed ADDRESS predictions; it does not change adapter labels.",
  excludedFromScoring: "Predictions outside corpus.scoredTypes are retained in observations but have no complete gold annotation in this scoped set.",
  hardware: {cpu: os.cpus()[0]?.model, platform: process.platform, arch: process.arch, totalMemoryBytes: os.totalmem()},
  observations: [], externalRequests: [], pageErrors: [], passedContracts: false,
};
const profile = await mkdtemp(path.join(os.tmpdir(), "datafog-browser-detection-"));
let context;
try {
  context = await chromium.launchPersistentContext(profile, {channel: "chromium", headless: true,
    args: ["--enable-automation", `--disable-extensions-except=${artifacts.dist}`, `--load-extension=${artifacts.dist}`]});
  context.on("request", request => { if (/^https?:/i.test(request.url())) receipt.externalRequests.push(request.url()); });
  context.on("page", page => page.on("pageerror", error => receipt.pageErrors.push(error.message)));
  let worker = context.serviceWorkers()[0];
  if (!worker) worker = await context.waitForEvent("serviceworker", {timeout: 30000});
  const page = await context.newPage();
  await page.goto(`chrome-extension://${new URL(worker.url()).hostname}/qualification.html`);
  await page.waitForFunction(() => Boolean(window.browserQualification), {timeout: 30000});
  receipt.browser = {version: context.browser().version(), channel: "Playwright Chromium", extension: true, manifest: await page.evaluate(() => chrome.runtime.getManifest())};
  assert.equal(receipt.browser.manifest.manifest_version, 3);
  receipt.descriptor = await page.evaluate(options => window.browserQualification.open(options), corpus.model);
  assert.deepEqual(receipt.descriptor.labels, corpus.model.labels);
  for (const record of corpus.records) {
    const start = performance.now();
    const result = await page.evaluate(text => window.browserQualification.scan(text), record.text);
    for (const finding of result.findings) validateSpan(record.text, finding);
    receipt.observations.push({id: record.id, durationMs: performance.now() - start, ...result});
    console.log(`Scanned ${record.id}: ${result.findings.length} findings`);
  }
  receipt.scores = scoreCorpus(corpus, receipt.observations);
  await page.evaluate(() => window.browserQualification.close());
  assert.deepEqual(receipt.externalRequests, []);
  assert.deepEqual(receipt.pageErrors, []);
  receipt.passedContracts = true;
  console.log(JSON.stringify(receipt.scores.shipping.exact.totals, null, 2));
} catch (error) {
  receipt.failure = {message: error.message};
  process.exitCode = 1;
} finally {
  await context?.close();
  await rm(profile, {recursive: true, force: true});
  const file = path.join(output, "receipt.json");
  await writeFile(file, JSON.stringify(receipt, null, 2) + "\n");
  const observations = receipt.observations.map(value => JSON.stringify(value)).join("\n") + "\n";
  const {observations: _observations, scores, ...summary} = receipt;
  summary.observationsSha256 = createHash("sha256").update(observations).digest("hex");
  if (scores) summary.scores = Object.fromEntries(Object.entries(scores).map(([view, modes]) => [view,
    Object.fromEntries(Object.entries(modes).map(([mode, value]) => [mode, value.totals]))]));
  await writeFile(path.join(output, "observations.jsonl"), observations);
  await writeFile(path.join(output, "summary.json"), JSON.stringify(summary, null, 2) + "\n");
  console.log(`Browser detection preparation receipt: ${file}`);
}
