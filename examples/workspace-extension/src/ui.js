import { client, safeCode } from "./rpc.js";

const broker = client(new Worker(new URL("./broker.js", import.meta.url), { type: "module" }));
const element = id => document.getElementById(id);
let findings = [];
let plan;
let generation = 0;
let restorationGeneration = 0;
let operations = 0;
const status = text => { element("status").textContent = text; };
function clearPreview() { plan = undefined; element("preview").textContent = ""; element("approve").disabled = true; }
function clearDraft() { element("source").value = ""; findings = []; element("selection-count").textContent = "0 selections"; clearPreview(); }
function clearViews() { clearDraft(); element("sink").textContent = ""; element("restore-input").value = ""; element("restored").textContent = ""; element("password").value = ""; }
async function perform(task) {
  const g = generation;
  operations++; renderBusy();
  try { await task(g); } catch (error) { if (g === generation) status(safeCode(error)); }
  finally { operations--; renderBusy(); }
}
function renderBusy() {
  for (const id of ["password", "create", "unlock", "mark", "prepare", "restore"]) element(id).disabled = operations > 0;
  element("approve").disabled = operations > 0 || plan === undefined;
}
function invalidateDraft() {
  generation++; findings = []; clearPreview(); element("selection-count").textContent = "0 selections";
  void broker.call("cancel").catch(error => status(safeCode(error)));
}
element("source").addEventListener("input", invalidateDraft);
element("entity-type").addEventListener("input", invalidateDraft);
element("restore-input").addEventListener("input", () => { restorationGeneration++; element("restored").textContent = ""; });
for (const name of ["create", "unlock"]) {
  element(name).addEventListener("click", () => {
    const password = element("password").value;
    generation++; clearViews();
    void perform(async g => {
    const result = await broker.call(name, [password]);
    if (g === generation) status(result.status);
    });
  });
}
element("mark").addEventListener("click", () => perform(async g => {
  const source = element("source");
  const finding = await broker.call("select", [source.value, source.selectionStart, source.selectionEnd, element("entity-type").value]);
  if (g !== generation) return;
  findings.push(finding); element("selection-count").textContent = `${findings.length} selections`; clearPreview(); status("selection_added");
}));
element("prepare").addEventListener("click", () => perform(async g => {
  const result = await broker.call("prepare", [element("source").value, findings]);
  if (g !== generation) return;
  plan = result.id; element("preview").textContent = result.preview.text; element("approve").disabled = false; status("preview_ready");
}));
element("approve").addEventListener("click", () => perform(async g => {
  element("approve").disabled = true;
  try {
    const result = await broker.call("approve", [plan]);
    if (g !== generation) return;
    clearDraft(); element("sink").textContent = result.text; status(result.status);
  } catch (error) {
    if (g === generation && ["save_uncertain", "workspace_conflict", "invalid_envelope", "closed_workspace", "cancelled", "stale_approval"].includes(error.code)) clearDraft();
    else if (g === generation && plan !== undefined) element("approve").disabled = false;
    throw error;
  }
}));
for (const name of ["lock", "cancel"]) element(name).addEventListener("click", () => {
  generation++; clearViews();
  void perform(async g => { const result = await broker.call(name); if (g === generation) status(result.status); });
});
element("restore").addEventListener("click", () => perform(async g => {
  const restoration = restorationGeneration;
  element("restored").textContent = "";
  const result = await broker.call("restore", [element("restore-input").value]);
  if (g === generation && restoration === restorationGeneration) { element("restored").textContent = result.text; status(result.status); }
}));
clearViews();
await broker.call("status"); status("ready");

/** Fault synchronization for this non-shipping synthetic harness only. */
export function testCommand(command, value) { return broker.call("test", [command, value]); }
