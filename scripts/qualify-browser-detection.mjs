import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {createHash} from "node:crypto";
import {mkdir, mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {createRequire} from "node:module";
import os from "node:os";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {scoreCorpus, validateSpan} from "./browser-detection-scoring.mjs";
import {validateSdkSpan} from "./browser-detection-contract.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const harness = path.join(root, "examples/workspace-extension");
const requireHarness = createRequire(path.join(harness, "package.json"));
const {chromium} = requireHarness("playwright");
const {build} = requireHarness("esbuild");
const corpusPath = path.join(root, "fixtures/browser-detection/qualification-v1.json");
const corpus = JSON.parse(await readFile(corpusPath));
const coreSdk = process.env.DATAFOG_CORE_SDK_QUALIFICATION === "1";
const output = process.env.DATAFOG_QUALIFICATION_OUTPUT || path.join(root, "target/browser-detection", new Date().toISOString().replaceAll(/[:.]/g, "-"));
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
  hasModel: () => Boolean(model),
};`, resolveDir: path.join(artifacts.output, "consumer"), sourcefile: "qualification-entry.mjs"},
  outfile: path.join(artifacts.dist, "qualification-entry.js"), bundle: true, format: "esm", platform: "browser", target: "chrome120", nodePaths: [modules], legalComments: "eof",
});
await build({entryPoints:[path.join(root,"scripts/browser-detection-workspace-entry.mjs")],
  outfile:path.join(artifacts.dist,"qualification-workspace.js"),bundle:true,format:"esm",platform:"browser",
  target:"chrome120",nodePaths:[modules],legalComments:"eof"});
await writeFile(path.join(artifacts.dist,"qualification.html"), '<!doctype html><meta charset="utf-8"><title>Synthetic browser qualification</title><output id="workflow-sink"></output><script type="module" src="qualification-entry.js"></script><script type="module" src="qualification-workspace.js"></script>');
const digest = async file => createHash("sha256").update(await readFile(file)).digest("hex");
const inputs = ["scripts/qualify-browser-detection.mjs", "scripts/browser-detection-workspace-entry.mjs", "scripts/browser-detection-scoring.mjs", "scripts/browser-detection-contract.mjs", "scripts/test-browser-detection-scoring.mjs", "fixtures/browser-detection/qualification-v1.json", "bindings/wasm/models/model.mjs", "bindings/wasm/package-lock.json", "examples/workspace-extension/package-lock.json"];
const receipt = {
  kind: "shipping-adapter-current-measurement", qualificationComplete: false,
  featureDependencies: {issues:[141,144,145,146],status:"merged and exercised"},
  pendingReview:["agreed pilot corpus", "generic unlabelled account coverage", "launch bar and remaining misses"], syntheticOnly: true,
  sourceCommit: artifacts.sourceCommit, sourceDirty: artifacts.sourceDirty,
  inputs: Object.fromEntries(await Promise.all(inputs.map(async file => [file, await digest(path.join(root, file))]))),
  model: artifacts.model, package: {...artifacts.tarball, file: path.basename(artifacts.tarball.file)},
  versions: artifacts.versions, consumerLockSha256: artifacts.consumerLockSha256,
  assets: artifacts.assets, extensionFiles: artifacts.extensionFiles,
  qualificationEntrySha256: await digest(path.join(artifacts.dist, "qualification-entry.js")),
  qualificationFiles:Object.fromEntries(await Promise.all(["qualification-entry.js","qualification-workspace.js","qualification.html"].map(async file=>[file,await digest(path.join(artifacts.dist,file))]))),
  options: corpus.model, config: {},
  matching: {exact: "same type and both byte boundaries", overlap: "same type and positive byte overlap; maximum one-to-one matching; diagnostic, not full-span coverage"},
  addressContract: "ADDRESS is full-span shipping gold. STREET_ADDRESS is a separate street-line diagnostic using untrimmed ADDRESS predictions; it does not change adapter labels.",
  excludedFromScoring: "Predictions outside corpus.scoredTypes are retained in observations but have no complete gold annotation in this scoped set.",
  hardware: {cpu: os.cpus()[0]?.model, platform: process.platform, arch: process.arch, totalMemoryBytes: os.totalmem()},
  observations: [], externalRequests: [], pageErrors: [], passedContracts: false,
};
if (coreSdk) {
  receipt.kind = "core-sdk-browser-current-measurement";
  receipt.scope = "Fresh packed Core SDK in a synthetic Chromium MV3 reference host; not shipping-extension acceptance";
  receipt.pendingReview = ["Shipping extension corpus, accuracy targets, and product acceptance belong to the extension release"];
  receipt.extensionAcceptance = false;
}
const profile = await mkdtemp(path.join(os.tmpdir(), "datafog-browser-detection-"));
let context;
try {
  context = await chromium.launchPersistentContext(path.join(profile,"original"), {channel: "chromium", headless: true,
    args: ["--enable-automation", `--disable-extensions-except=${artifacts.dist}`, `--load-extension=${artifacts.dist}`]});
  context.on("request", request => { if (/^https?:/i.test(request.url())) receipt.externalRequests.push(request.url()); });
  context.on("page", page => page.on("pageerror", error => receipt.pageErrors.push(error.message)));
  let worker = context.serviceWorkers()[0];
  if (!worker) worker = await context.waitForEvent("serviceworker", {timeout: 30000});
  let page = await context.newPage();
  const extensionOrigin=`chrome-extension://${new URL(worker.url()).hostname}`;
  await page.goto(`${extensionOrigin}/qualification.html`);
  await page.waitForFunction(() => Boolean(window.browserQualification && window.browserWorkspaceQualification), {timeout: 30000});
  async function call(name,...args) {return page.evaluate(([name,args])=>window.browserWorkspaceQualification[name](...args),[name,args]);}
  async function restart(name) {
    await context.close();
    context=await chromium.launchPersistentContext(path.join(profile,name),{channel:"chromium",headless:true,
      args:["--enable-automation",`--disable-extensions-except=${artifacts.dist}`,`--load-extension=${artifacts.dist}`]});
    context.on("request",request=>{if(/^https?:/i.test(request.url()))receipt.externalRequests.push(request.url());});
    context.on("page",opened=>opened.on("pageerror",error=>receipt.pageErrors.push(error.message)));
    let restarted=context.serviceWorkers()[0];if(!restarted)restarted=await context.waitForEvent("serviceworker",{timeout:30000});
    assert.equal(`chrome-extension://${new URL(restarted.url()).hostname}`,extensionOrigin);
    page=await context.newPage();await page.goto(`${extensionOrigin}/qualification.html`);
    await page.waitForFunction(()=>Boolean(window.browserQualification&&window.browserWorkspaceQualification),{timeout:30000});
  }
  receipt.browser = {version: context.browser().version(), channel: "Playwright Chromium", extension: true, manifest: await page.evaluate(() => chrome.runtime.getManifest())};
  assert.equal(receipt.browser.manifest.manifest_version, 3);
  receipt.descriptor = await page.evaluate(options => window.browserQualification.open(options), corpus.model);
  assert.deepEqual(receipt.descriptor.labels, corpus.model.labels);
  for (const record of corpus.records) {
    const start = performance.now();
    const result = await page.evaluate(text => window.browserQualification.scan(text), record.text);
    for (const finding of result.findings) validateSdkSpan(record.text, finding);
    receipt.observations.push({id: record.id, durationMs: performance.now() - start, ...result});
    console.log(`Scanned ${record.id}: ${result.findings.length} findings`);
  }
  receipt.scores = scoreCorpus(corpus, receipt.observations);
  await page.evaluate(() => window.browserQualification.close());
  const password="synthetic-g2-only-passphrase";
  const initial=await call("create",password),recoveryCode=initial.recoveryCode;
  const detectorRecord=corpus.records.find(record=>record.id==="person-01");
  const observed=receipt.observations.find(record=>record.id===detectorRecord.id);
  const scannerFindings=observed.findings.map(f=>({entityType:f.entityType,matchedText:f.matchedText,
    range:{unit:"utf8_bytes",...f.byteRange},detectorName:f.detectorName,detectorVersion:f.detectorVersion,
    source:"scanner",needsReview:true,...(f.confidence===undefined?{}:{confidence:f.confidence})}));
  const scannerInput={text:detectorRecord.text,scannerFindings,userSelections:[]};
  const before=await call("fingerprint");
  let prepared=await call("stage",[scannerInput]);
  assert(prepared.previews[0].transformations.length>0);
  assert(!prepared.previews[0].text.includes("John Smith"));
  assert.equal((await call("report",prepared.previews[0].text)).text,null);
  assert(prepared.previews[0].transformations.every(value=>value.source==="scanner"&&value.needsReview));
  assert.equal((await call("query","John Smith")).findings.length,0);
  assert.equal(await page.locator("#workflow-sink").textContent(),"");
  const rejected=await page.evaluate(async()=>{try{await window.browserWorkspaceQualification.approve(false);return "success";}catch(error){return error.code;}});
  assert.equal(rejected,"cancelled");assert.deepEqual(await call("fingerprint"),before);
  prepared=await call("stage",[scannerInput]);
  const aborted=await page.evaluate(async()=>{try{await window.browserWorkspaceQualification.approve(true,true);return "success";}catch(error){return error.code;}});
  assert.equal(aborted,"storage_failure");assert.deepEqual(await call("fingerprint"),before);
  assert.equal(await page.locator("#workflow-sink").textContent(),"");
  assert.equal((await call("query","John Smith")).findings.length,0);
  const scannerSaved=await call("approve",true);
  assert.equal(await call("restore",scannerSaved.previews[0].text),detectorRecord.text);
  const eligible=corpus.records.filter(record=>record.gold.length);
  const approved=[];
  // Gold seeds are explicit fixture-approved manual selections, separate from
  // detection accuracy. They never alter raw detector observations or scores.
  for(let offset=0;offset<eligible.length;offset+=32) {
    const rows=eligible.slice(offset,offset+32);
    const inputs=rows.map(record=>({text:record.text,scannerFindings:[],userSelections:record.gold.map(f=>({
      entityType:f.entityType,matchedText:f.matchedText,range:{unit:"utf8_bytes",...f.byteRange},source:"user",needsReview:true}))}));
    const fingerprint=await call("fingerprint");await call("stage",inputs);
    assert.deepEqual(await call("fingerprint"),fingerprint);assert.equal(await page.locator("#workflow-sink").textContent(),"");
    const saved=await call("approve",true);
    for(let index=0;index<rows.length;index++)approved.push({id:rows[index].id,text:rows[index].text,preview:saved.previews[index].text});
  }
  async function recallCheck(name) {
    let checked=0;const misses=[],fingerprint=await call("fingerprint");
    for(const record of eligible)for(const gold of record.gold) {
      const prefix="👋 é follow-up: ",text=prefix+gold.matchedText+" .";
      const result=await call("query",text),start=Buffer.byteLength(prefix),end=start+Buffer.byteLength(gold.matchedText);
      for(const found of result.findings) {
        assert.equal(found.finding.source,"recall");assert.equal(found.finding.needsReview,true);
        validateSdkSpan(text,{...found.finding,byteRange:found.byteRange,codepointRange:found.codepointRange,utf16Range:found.utf16Range});
      }
      if(!result.findings.some(found=>found.finding.entityType===gold.entityType&&found.byteRange.start===start&&found.byteRange.end===end))misses.push({id:record.id,entityType:gold.entityType});
      checked++;
    }
    for(const record of approved)assert.equal(await call("restore",record.preview),record.text);
    assert.deepEqual(misses,[],`${name}: eligible approved-value recall misses`);
    assert.deepEqual(await call("fingerprint"),fingerprint,"recall/restore changed committed state");
    return {name,checked,misses,restoredRecords:approved.length,readOnly:true};
  }
  const workflow={scope:"synthetic explicit approvals; detector accuracy remains the independent raw 64-record measurement",checks:[],recall:[]};
  workflow.checks.push("actual scanner findings require approval, aborted IndexedDB save releases nothing, explicit retry saves before activation");
  workflow.recall.push(await recallCheck("after explicit saved approvals"));
  const normalized=await call("query","JOHN\tSMITH"),blocked=await call("query","John Smithson");
  assert.equal(normalized.findings.length,1);assert.equal(blocked.findings.length,0);
  workflow.checks.push("case/whitespace recall and no substring expansion");
  assert.equal(await call("noStoredPlaintext",["John Smith","alice@example.com"]),true);
  const backup=await call("backup",password),head=await call("head");
  await call("close");await restart("original");
  assert.equal(await page.evaluate(()=>window.browserQualification.hasModel()),false);
  assert.deepEqual(await call("open",password),head);
  workflow.recall.push(await recallCheck("browser process restart without model"));
  for(const method of ["password","recovery"]) {
    await call("close");await restart(`import-${method}`);
    if(method==="password") {
      const wrong=await page.evaluate(async bytes=>{try{await window.browserWorkspaceQualification.recover(bytes,"incorrect synthetic password","password");return "success";}catch(error){return error.code;}},backup);
      assert.equal(wrong,"invalid_credentials");
    }
    const imported=await call("recover",backup,method==="password"?password:recoveryCode,method);
    assert.equal(imported.profile,2);assert.equal(imported.freshKey,true);assert.equal(imported.freshNonce,true);
    assert.notDeepEqual(imported.head.commitId,head.commitId);
    workflow.recall.push(await recallCheck(`${method} backup import, fresh encryption and head`));
    await call("close");await restart(`import-${method}`);
    await call("open",method==="password"?password:recoveryCode,method);
    workflow.recall.push(await recallCheck(`${method} recovered browser restart`));
  }
  workflow.checks.push("stronger password-reauthenticated export, both import credentials, fresh keys/nonces/head and reopened permanent aliases");
  workflow.passed=true;receipt.workspaceWorkflow=workflow;
  backup.fill(0);await call("close");
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
