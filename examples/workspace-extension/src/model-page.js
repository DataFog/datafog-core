import {init, Workspace} from "@datafog/wasm";
import {ModelSession} from "@datafog/wasm/models";
import {createVault, openVault, encryptCandidate, randomId} from "./vault.js";
import {readRecords, saveRecord} from "./storage.js";

await init();
let model, workspace, host, pending;
let stalePlans = [];
const status = value => { document.querySelector("#status").textContent = value; };
async function timed(operation) {
  let last = performance.now(), maximum = 0, ticks = 0;
  const timer = setInterval(() => {
    const now = performance.now(); maximum = Math.max(maximum, now - last); last = now; ticks++;
  }, 8);
  const start = performance.now();
  try {
    const value = await operation();
    const durationMs = performance.now() - start;
    await new Promise(resolve => setTimeout(resolve, 20));
    return {value, durationMs, maximumTimerGapMs: maximum, timerTicks: ticks};
  } finally { clearInterval(timer); }
}
async function openModel(variant = "valid") {
  const measured = await timed(async () => {
    model = await ModelSession.open({
      modelBaseUrl: chrome.runtime.getURL(variant === "bad-model" ? "bad-model/" : "models/"),
      wasmPaths: chrome.runtime.getURL(variant === "missing-wasm" ? "missing-runtime/" : "ort/"),
      numThreads: 1,
    });
    return model.describe();
  });
  status("model_ready"); return measured;
}
async function openWorkspace(password) {
  const {vault, workspace: record} = await readRecords();
  if (vault) {
    const opened = await openVault(password, vault, record);
    try { workspace = Workspace.openBundle(opened.bundle); }
    finally { opened.bundle.fill(0); }
    host = {vault, record, dataKey: opened.dataKey};
  } else {
    workspace = Workspace.create(randomId(), randomId());
    const created = await createVault(password, workspace.namespace(), workspace.exportBundle(), workspace.head());
    await saveRecord({...created, creating: true, authorized: () => true});
    host = {vault: created.vault, record: created.record, dataKey: created.dataKey};
  }
  return workspace.head();
}
async function scan(text) {
  return timed(() => model.scan(text, {entities: ["PERSON", "EMAIL"]}));
}
function workspaceFindings(findings) {
  return findings.map(finding => ({
    entityType: finding.entityType, matchedText: finding.matchedText,
    range: {unit: "utf8_bytes", ...finding.byteRange},
    detectorName: finding.detectorName, detectorVersion: finding.detectorVersion,
    ...(finding.confidence === undefined ? {} : {confidence: finding.confidence}),
  }));
}
async function prepare(text) {
  const detected = await scan(text);
  const findings = workspaceFindings(detected.value.findings);
  const change = workspace.prepare(text, findings, randomId());
  pending = {change, preview: change.preview()};
  document.querySelector("#sink").textContent = "";
  return {...detected, preview: pending.preview, heads: change.heads()};
}
async function approve(abort = false) {
  if (!pending) throw new Error("no pending approval");
  const {change, preview} = pending;
  const heads = change.heads();
  const record = await encryptCandidate(host.dataKey, host.record, heads.candidateHead, change.exportCandidateBundle());
  await saveRecord({vault: host.vault, record, expectedHead: heads.expectedHead,
    expectedGeneration: host.vault.accessGeneration, authorized: () => true, abort});
  workspace.activateSaved(change, heads.expectedHead, heads.candidateHead);
  host.record = record; pending = undefined;
  document.querySelector("#sink").textContent = preview.text;
  return {text: preview.text, restored: workspace.restore(preview.text), head: workspace.head()};
}
async function closeModel() { await model?.close(); status("model_closed"); }
window.modelProof = {
  openModel, openWorkspace, scan, prepare, approve, closeModel,
  descriptor: () => model.describe(), restore: text => workspace.restore(text),
  restoreReport: text => workspace.restoreReport(text), records: readRecords,
  hasModel: () => Boolean(model),
  seedStale(text, findings) {
    stalePlans = Array.from({length: 31}, () => workspace.prepare(text, workspaceFindings(findings), randomId()));
    return stalePlans.length;
  },
  planReclamation() {
    const probes = [];
    try {
      probes.push(workspace.prepare("ordinary words", [], randomId()));
      probes.push(workspace.prepare("ordinary words", [], randomId()));
      return {accepted: true};
    } catch (error) { return {accepted: false, code: error.code}; }
    finally { for (const plan of probes) plan.discard(); stalePlans = []; }
  },
  sourcePreflight() {
    try {
      const plan = workspace.prepare("👋 e\u0301 literal [[my_note]]", [], randomId());
      plan.discard(); return {blocked: false};
    } catch (error) { return {blocked: true, code: error.code, markerIssues: error.markerIssues}; }
  },
  async closeWorkspace() { workspace?.close(); workspace = undefined; host = undefined; pending = undefined; },
};
document.querySelector("#load").onclick = () => openModel().catch(error => status(error.code || "failed"));
document.querySelector("#close").onclick = () => closeModel().catch(error => status(error.code || "failed"));
status("ready");
