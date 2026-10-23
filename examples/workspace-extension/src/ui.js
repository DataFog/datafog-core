import { client, safeCode, HostError } from "./rpc.js";
import {MAX_BACKUP_BYTES} from "./backup.js";

const broker = client(new Worker(new URL("./broker.js", import.meta.url), { type: "module" }));
const element = id => document.getElementById(id);
let findings = [];
let queued = [];
let plan;
let importPlan;
let generation = 0;
let restorationGeneration = 0;
let operations = 0;
const status = text => { element("status").textContent = text; };
function clearPreview() { plan = undefined; element("preview").textContent = ""; element("approve").disabled = true; }
function clearCurrent() { element("source").value = ""; findings = []; element("selection-count").textContent = "0 selections"; }
function clearDraft() { clearCurrent(); queued = []; renderInputs(); clearPreview(); element("batch-error").textContent = ""; }
function renderInputs() {
  element("inputs").replaceChildren();
  queued.forEach((input, index) => {
    const item = document.createElement("li");
    item.textContent = `Input ${index + 1}`;
    const remove = document.createElement("button");
    remove.textContent = `Remove input ${index + 1}`;
    remove.addEventListener("click", () => {
      generation++; queued = queued.filter(value => value !== input);
      clearPreview(); element("batch-error").textContent = ""; renderInputs();
      void perform(async g => { await broker.call("cancel"); if (g === generation) status("input_removed"); });
    });
    item.append(remove); element("inputs").append(item);
  });
}
function showInputs(texts) { return texts.length === 1 ? texts[0] : texts.map((text, i) => `Input ${i + 1}\n${text}`).join("\n\n"); }
function clearImportPreview() { importPlan = undefined; element("backup-preview").textContent = ""; element("backup-approve").disabled = true; }
function hideRecovery() { element("recovery-created").value = ""; element("recovery-notice").hidden = true; }
function clearViews() {
  clearDraft(); clearImportPreview(); hideRecovery();
  element("sink").textContent = ""; element("restore-input").value = ""; element("restored").textContent = "";
  for (const id of ["password", "backup-credential", "backup-file"]) element(id).value = "";
}
async function perform(task) {
  const g = generation;
  operations++; renderBusy();
  try { await task(g); } catch (error) {
    if (g === generation) {
      status(safeCode(error));
      const guidance = error.code === "invalid_marker" ? " Use the exact pseudonym from this unlocked workspace. Unknown, foreign and unsaved markers are blocked." : error.code === "workspace_conflict" ? " The workspace changed or this restore destination is not empty. No existing workspace was overwritten. Use an empty profile for backup restore." : error.code === "invalid_backup" ? " The backup is damaged or unsupported. Nothing was imported." : error.code === "recovery_unavailable" ? " This disposable older vault has no recovery slot. Backup export requires a newly created synthetic vault." : "";
      element("batch-error").textContent = (Number.isSafeInteger(error.inputIndex) ? `Input ${error.inputIndex + 1} rejected. Remove or correct it, then prepare a new preview.` : "") + guidance;
    }
  }
  finally { operations--; renderBusy(); }
}
function renderBusy() {
  if (importPlan && importPlan.generation !== generation) clearImportPreview();
  for (const id of ["password", "unlock-method", "create", "unlock", "mark", "prepare", "restore", "add-input", "backup-export", "backup-file", "backup-method", "backup-credential", "backup-review"]) element(id).disabled = operations > 0;
  element("approve").disabled = operations > 0 || plan === undefined;
  element("backup-approve").disabled = operations > 0 || importPlan === undefined;
}
function invalidateDraft() {
  generation++; findings = []; clearPreview(); clearImportPreview(); element("selection-count").textContent = "0 selections"; element("batch-error").textContent = "";
  void broker.call("cancel").catch(error => status(safeCode(error)));
}
element("source").addEventListener("input", invalidateDraft);
element("entity-type").addEventListener("input", invalidateDraft);
element("restore-input").addEventListener("input", () => { restorationGeneration++; element("restored").textContent = ""; });
for (const name of ["create", "unlock"]) {
  element(name).addEventListener("click", () => {
    const password = element("password").value;
    const method = element("unlock-method").value;
    generation++; clearViews();
    void perform(async g => {
    const result = await broker.call(name, [password, method]);
    if (g === generation) {
      if (result.recoveryCode) { element("recovery-created").value = result.recoveryCode; element("recovery-notice").hidden = false; }
      status(result.status);
    }
    });
  });
}
element("recovery-dismiss").addEventListener("click", hideRecovery);
element("backup-export").addEventListener("click", () => perform(async g => {
  const bytes = await broker.call("exportBackup");
  try {
    if (g !== generation) return;
    const url = URL.createObjectURL(new Blob([bytes], {type: "application/octet-stream"}));
    try { const link = document.createElement("a"); link.href = url; link.download = "datafog-workspace.dfbackup"; link.click(); }
    finally { setTimeout(() => URL.revokeObjectURL(url), 0); }
    status("backup_exported");
  } finally { bytes.fill(0); }
}));
for (const [id, event] of [["backup-file", "change"], ["backup-method", "change"], ["backup-credential", "input"]]) {
  element(id).addEventListener(event, () => {
    const staged = importPlan !== undefined; clearImportPreview();
    if (staged) { generation++; void broker.call("cancel").catch(error => status(safeCode(error))); }
  });
}
element("backup-review").addEventListener("click", () => {
  const file = element("backup-file").files[0], credential = element("backup-credential").value, method = element("backup-method").value;
  element("backup-credential").value = ""; element("backup-file").value = ""; clearImportPreview();
  void perform(async g => {
    if (!file) throw new HostError("invalid_backup");
    if (file.size > MAX_BACKUP_BYTES) throw new HostError("resource_limit");
    const bytes = new Uint8Array(await file.arrayBuffer());
    try {
      if (g !== generation) return;
      const result = await broker.call("prepareImport", [bytes, credential, method]);
      if (g !== generation) return;
      importPlan = {...result, generation: g};
      element("backup-preview").textContent = `Saved workspace ${result.workspaceTag}\nExported ${result.exportedAt}\nSaved revision ${result.revision}\nRestore saved mappings/settings into this empty profile.`;
      status("backup_preview_ready");
    } finally { bytes.fill(0); }
  });
});
element("backup-approve").addEventListener("click", () => perform(async g => {
  const approved = importPlan; clearImportPreview();
  const result = await broker.call("approveImport", [approved.id]);
  if (g === generation) { clearViews(); status(result.status); }
}));
element("mark").addEventListener("click", () => perform(async g => {
  const source = element("source");
  const finding = await broker.call("select", [source.value, source.selectionStart, source.selectionEnd, element("entity-type").value]);
  if (g !== generation) return;
  findings.push(finding); element("selection-count").textContent = `${findings.length} selections`; clearPreview(); status("selection_added");
}));
element("add-input").addEventListener("click", () => {
  if (queued.length >= 32) { status("resource_limit"); return; }
  const input = { text: element("source").value, findings };
  generation++; clearPreview();
  void perform(async g => {
    await broker.call("cancel");
    if (g !== generation) return;
    queued.push(input); clearCurrent(); renderInputs(); element("batch-error").textContent = ""; status("input_added");
  });
});
element("prepare").addEventListener("click", () => perform(async g => {
  clearPreview(); element("batch-error").textContent = "";
  const inputs = queued.map(input => ({text: input.text, findings: input.findings}));
  const text = element("source").value;
  if (text.length || findings.length || inputs.length === 0) inputs.push({text, findings});
  const result = await broker.call("prepare", [inputs]);
  if (g !== generation) return;
  plan = result.id; element("preview").textContent = showInputs(result.previews.map(preview => preview.text)); element("approve").disabled = false; status("preview_ready");
}));
element("approve").addEventListener("click", () => perform(async g => {
  element("approve").disabled = true;
  try {
    const result = await broker.call("approve", [plan]);
    if (g !== generation) return;
    clearDraft(); element("sink").textContent = showInputs(result.texts); status(result.status);
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
