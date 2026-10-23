import { client, safeCode, HostError } from "./rpc.js";
import { importFiles, flattenDocuments, formatDocuments, cellSelection } from "./documents.js";
import {MAX_BACKUP_BYTES} from "./backup.js";

const broker = client(new Worker(new URL("./broker.js", import.meta.url), { type: "module" }));
const element = id => document.getElementById(id);
let findings = [];
let queued = [];
let intakeController;
let reviewDocument;
let reviewField;
let plan;
let importPlan;
let generation = 0;
let restorationGeneration = 0;
let operations = 0;
const status = text => { element("status").textContent = text; };
function clearPreview() { plan = undefined; element("preview").textContent = ""; element("approve").disabled = true; }
function clearCurrent() { element("source").value = ""; findings = []; element("selection-count").textContent = "0 selections"; }
function stopIntake() { intakeController?.abort(); intakeController = undefined; element("file-progress").textContent = ""; }
function clearCell() { reviewField = undefined; element("csv-cell").value = ""; element("cell-selections").textContent = ""; }
function clearTableReview() { reviewDocument = undefined; clearCell(); element("csv-review").hidden = true; element("xlsx-location").hidden = true; element("pdf-location").hidden = true; }
function clearDraft() { stopIntake(); clearCurrent(); clearTableReview(); queued = []; element("files").value = ""; renderInputs(); clearPreview(); element("batch-error").textContent = ""; }
function renderInputs() {
  element("inputs").replaceChildren();
  queued.forEach((input, index) => {
    const item = document.createElement("li");
    item.textContent = `Input ${index + 1}${input.label ? `: ${input.label}` : ""}${input.kind === "csv" ? `: CSV, ${input.widths.length} records` : input.kind === "xlsx" ? `: XLSX, ${input.sheets.length} sheets` : input.kind === "pdf" ? `: PDF, ${input.pages.length} pages` : input.kind === "rejected" ? `: ${input.error}` : ""}`;
    if (["csv", "xlsx", "pdf"].includes(input.kind)) {
      const review = document.createElement("button"); review.textContent = `Review input ${index + 1}`;
      review.addEventListener("click", () => {
        reviewDocument = input; clearCell(); element("csv-review").hidden = false;
        element("csv-location").hidden = input.kind !== "csv"; element("xlsx-location").hidden = input.kind !== "xlsx";
        element("pdf-location").hidden = input.kind !== "pdf"; element("pdf-page").value = "1";
        element("csv-row").value = "1"; element("csv-column").value = "1";
        element("xlsx-sheet").value = "1"; element("xlsx-address").value = "A1"; element("xlsx-component").value = "value";
      });
      item.append(review);
    }
    const remove = document.createElement("button");
    remove.textContent = `Remove input ${index + 1}`;
    remove.addEventListener("click", () => {
      generation++; stopIntake(); if (reviewDocument === input) clearTableReview(); queued = queued.filter(value => value !== input);
      clearPreview(); element("batch-error").textContent = ""; renderInputs();
      void perform(async g => { await broker.call("cancel"); if (g === generation) status("input_removed"); });
    });
    item.append(remove); element("inputs").append(item);
  });
}
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
      const location = Number.isSafeInteger(error.pageIndex) ? `, page ${error.pageIndex + 1}` : Number.isSafeInteger(error.sheetIndex) ? `, sheet ${error.sheetIndex + 1}${error.cellAddress ? `, cell ${error.cellAddress}` : ""}, ${error.component}` : Number.isSafeInteger(error.rowIndex) ? `, record ${error.rowIndex + 1}, column ${error.columnIndex + 1}` : "";
      const guidance = error.code === "invalid_marker" ? " Use the exact full pseudonym from this unlocked workspace. Unknown, damaged, foreign and unsaved markers are blocked." : error.code === "workspace_conflict" ? " The workspace changed or this restore destination is not empty. No existing workspace was overwritten. Use an empty profile for backup restore." : error.code === "invalid_backup" ? " The backup is damaged or unsupported. Nothing was imported." : error.code === "recovery_unavailable" ? " This disposable older vault has no recovery slot. Backup export requires a newly created synthetic vault." : error.code === "formulas_not_supported" ? " Supply a values-only XLSX copy or CSV export." : ["unsupported_xlsx", "protected_xlsx", "invalid_xlsx"].includes(error.code) ? " Supply an unencrypted values-only XLSX copy or CSV export." : ["scanned_pdf", "unsupported_pdf", "protected_pdf", "invalid_pdf"].includes(error.code) ? " Supply an unencrypted text-only PDF without images or unsupported features, or a CSV export." : "";
      element("batch-error").textContent = (Number.isSafeInteger(error.inputIndex) ? `Input ${error.inputIndex + 1}${location} rejected. Remove or correct it, then prepare a new preview.` : "") + guidance;
    }
  }
  finally { operations--; renderBusy(); }
}
function renderBusy() {
  if (importPlan && importPlan.generation !== generation) clearImportPreview();
  for (const id of ["password", "unlock-method", "create", "unlock", "mark", "prepare", "restore", "add-input", "load-cell", "mark-cell", "clear-cell", "backup-export", "backup-file", "backup-method", "backup-credential", "backup-review"]) element(id).disabled = operations > 0;
  element("approve").disabled = operations > 0 || plan === undefined;
  element("backup-approve").disabled = operations > 0 || importPlan === undefined;
}
function invalidateDraft() {
  generation++; stopIntake(); findings = []; clearPreview(); clearImportPreview(); element("selection-count").textContent = "0 selections"; element("batch-error").textContent = "";
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
  generation++; stopIntake(); clearPreview();
  void perform(async g => {
    await broker.call("cancel");
    if (g !== generation) return;
    queued.push(input); clearCurrent(); renderInputs(); element("batch-error").textContent = ""; status("input_added");
  });
});
element("files").addEventListener("change", () => {
  const files = Array.from(element("files").files); element("files").value = "";
  generation++; stopIntake(); clearPreview();
  const controller = new AbortController(); intakeController = controller;
  void perform(async g => {
    try {
      await broker.call("cancel");
      if (g !== generation) return;
      if ((await broker.call("status")).status !== "unlocked") throw new HostError("locked");
      const imported = await importFiles(files, queued, controller.signal, (index, count, stage) => {
        if (g === generation) element("file-progress").textContent = `${stage === "parsing" ? "Extracting" : "Reading"} file ${index} of ${count}`;
      });
      if (g !== generation) return;
      queued.push(...imported); renderInputs(); element("batch-error").textContent = ""; status("files_added");
    } finally {
      files.length = 0;
      if (intakeController === controller) { intakeController = undefined; element("file-progress").textContent = ""; }
    }
  });
});
element("load-cell").addEventListener("click", () => {
  clearCell();
  if (reviewDocument?.kind === "pdf") {
    const pageIndex = Number(element("pdf-page").value) - 1;
    const page = Number.isSafeInteger(pageIndex) && pageIndex >= 0 ? reviewDocument.pages[pageIndex] : undefined;
    if (!page) { status("invalid_cell"); return; }
    reviewField = reviewDocument.fields[page.fieldIndex];
  } else if (reviewDocument?.kind === "xlsx") {
    const sheetIndex = Number(element("xlsx-sheet").value) - 1;
    const sheet = Number.isSafeInteger(sheetIndex) && sheetIndex >= 0 ? reviewDocument.sheets[sheetIndex] : undefined;
    if (!sheet) { status("invalid_cell"); return; }
    const component = element("xlsx-component").value;
    if (component === "name") reviewField = reviewDocument.fields[sheet.nameIndex];
    else {
      const cell = sheet.cells.find(cell => cell.address === element("xlsx-address").value.trim().toUpperCase());
      if (cell) reviewField = reviewDocument.fields[component === "numberFormat" ? cell.formatIndex : cell.valueIndex];
    }
    if (!reviewField) { status("invalid_cell"); return; }
  } else {
    const row = Number(element("csv-row").value) - 1, column = Number(element("csv-column").value) - 1;
    if (!reviewDocument || !Number.isSafeInteger(row) || !Number.isSafeInteger(column) || row < 0 || column < 0 || row >= reviewDocument.widths.length || column >= reviewDocument.widths[row]) { status("invalid_cell"); return; }
    reviewField = reviewDocument.fields[reviewDocument.offsets[row] + column];
  }
  element("csv-cell").value = reviewField.text;
  element("cell-selections").textContent = `${reviewField.findings.length} selections`; status("cell_ready");
});
element("mark-cell").addEventListener("click", () => {
  if (!reviewField) { status("invalid_cell"); return; }
  const cell = reviewField, textarea = element("csv-cell"), start = textarea.selectionStart, end = textarea.selectionEnd;
  generation++; clearPreview();
  void perform(async g => {
    const original = cellSelection(cell.text, start, end);
    const finding = await broker.call("select", [cell.text, original.start, original.end, element("entity-type").value]);
    if (g !== generation) return;
    cell.findings.push(finding); element("cell-selections").textContent = `${cell.findings.length} selections`; status("selection_added");
  });
});
element("clear-cell").addEventListener("click", () => {
  if (!reviewField) return;
  generation++; clearPreview(); reviewField.findings = []; element("cell-selections").textContent = "0 selections";
  void perform(async g => { await broker.call("cancel"); if (g === generation) status("selections_cleared"); });
});
element("prepare").addEventListener("click", () => perform(async g => {
  clearPreview(); element("batch-error").textContent = "";
  const documents = [...queued];
  const text = element("source").value;
  if (text.length || findings.length || documents.length === 0) documents.push({text, findings});
  let flattened;
  try {
    flattened = flattenDocuments(documents);
    const result = await broker.call("prepare", [flattened.inputs]);
    if (g !== generation) return;
    const preview = formatDocuments(documents, result.previews.map(preview => preview.text));
    plan = {id: result.id, documents}; element("preview").textContent = preview; element("approve").disabled = false; status("preview_ready");
  } catch (error) {
    if (flattened && Number.isSafeInteger(error.inputIndex) && flattened.locations[error.inputIndex]) Object.assign(error, flattened.locations[error.inputIndex]);
    throw error;
  }
}));
element("approve").addEventListener("click", () => perform(async g => {
  element("approve").disabled = true;
  try {
    const approved = plan;
    const result = await broker.call("approve", [approved.id]);
    if (g !== generation) return;
    const output = formatDocuments(approved.documents, result.texts);
    clearDraft(); element("sink").textContent = output; status(result.status);
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
