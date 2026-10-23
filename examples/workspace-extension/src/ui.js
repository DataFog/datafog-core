import { client, safeCode, HostError } from "./rpc.js";
import { importCsvFiles, flattenDocuments, formatDocuments, cellSelection } from "./csv.js";

const broker = client(new Worker(new URL("./broker.js", import.meta.url), { type: "module" }));
const element = id => document.getElementById(id);
let findings = [];
let queued = [];
let intakeController;
let csvDocument;
let csvCell;
let plan;
let generation = 0;
let restorationGeneration = 0;
let operations = 0;
const status = text => { element("status").textContent = text; };
function clearPreview() { plan = undefined; element("preview").textContent = ""; element("approve").disabled = true; }
function clearCurrent() { element("source").value = ""; findings = []; element("selection-count").textContent = "0 selections"; }
function stopIntake() { intakeController?.abort(); intakeController = undefined; element("file-progress").textContent = ""; }
function clearCell() { csvCell = undefined; element("csv-cell").value = ""; element("cell-selections").textContent = ""; }
function clearCsvReview() { csvDocument = undefined; clearCell(); element("csv-review").hidden = true; }
function clearDraft() { stopIntake(); clearCurrent(); clearCsvReview(); queued = []; element("files").value = ""; renderInputs(); clearPreview(); element("batch-error").textContent = ""; }
function renderInputs() {
  element("inputs").replaceChildren();
  queued.forEach((input, index) => {
    const item = document.createElement("li");
    item.textContent = `Input ${index + 1}${input.label ? `: ${input.label}` : ""}${input.kind === "csv" ? `: CSV, ${input.widths.length} records` : input.kind === "rejected" ? `: ${input.error}` : ""}`;
    if (input.kind === "csv") {
      const review = document.createElement("button"); review.textContent = `Review input ${index + 1}`;
      review.addEventListener("click", () => {
        csvDocument = input; clearCell(); element("csv-review").hidden = false;
        element("csv-row").value = "1"; element("csv-column").value = "1";
      });
      item.append(review);
    }
    const remove = document.createElement("button");
    remove.textContent = `Remove input ${index + 1}`;
    remove.addEventListener("click", () => {
      generation++; stopIntake(); if (csvDocument === input) clearCsvReview(); queued = queued.filter(value => value !== input);
      clearPreview(); element("batch-error").textContent = ""; renderInputs();
      void perform(async g => { await broker.call("cancel"); if (g === generation) status("input_removed"); });
    });
    item.append(remove); element("inputs").append(item);
  });
}
function clearViews() { clearDraft(); element("sink").textContent = ""; element("restore-input").value = ""; element("restored").textContent = ""; element("password").value = ""; }
async function perform(task) {
  const g = generation;
  operations++; renderBusy();
  try { await task(g); } catch (error) {
    if (g === generation) {
      status(safeCode(error));
      element("batch-error").textContent = Number.isSafeInteger(error.inputIndex) ? `Input ${error.inputIndex + 1}${Number.isSafeInteger(error.rowIndex) ? `, record ${error.rowIndex + 1}, column ${error.columnIndex + 1}` : ""} rejected. Remove or correct it, then prepare a new preview.` : "";
    }
  }
  finally { operations--; renderBusy(); }
}
function renderBusy() {
  for (const id of ["password", "create", "unlock", "mark", "prepare", "restore", "add-input", "load-cell", "mark-cell", "clear-cell"]) element(id).disabled = operations > 0;
  element("approve").disabled = operations > 0 || plan === undefined;
}
function invalidateDraft() {
  generation++; stopIntake(); findings = []; clearPreview(); element("selection-count").textContent = "0 selections"; element("batch-error").textContent = "";
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
      const imported = await importCsvFiles(files, queued, controller.signal, (index, count) => {
        if (g === generation) element("file-progress").textContent = `Reading file ${index} of ${count}`;
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
  const row = Number(element("csv-row").value) - 1, column = Number(element("csv-column").value) - 1;
  if (!csvDocument || !Number.isSafeInteger(row) || !Number.isSafeInteger(column) || row < 0 || column < 0 || row >= csvDocument.widths.length || column >= csvDocument.widths[row]) { status("invalid_cell"); return; }
  csvCell = csvDocument.fields[csvDocument.offsets[row] + column];
  element("csv-cell").value = csvCell.text;
  element("cell-selections").textContent = `${csvCell.findings.length} selections`; status("cell_ready");
});
element("mark-cell").addEventListener("click", () => {
  if (!csvCell) { status("invalid_cell"); return; }
  const cell = csvCell, textarea = element("csv-cell"), start = textarea.selectionStart, end = textarea.selectionEnd;
  generation++; clearPreview();
  void perform(async g => {
    const original = cellSelection(cell.text, start, end);
    const finding = await broker.call("select", [cell.text, original.start, original.end, element("entity-type").value]);
    if (g !== generation) return;
    cell.findings.push(finding); element("cell-selections").textContent = `${cell.findings.length} selections`; status("selection_added");
  });
});
element("clear-cell").addEventListener("click", () => {
  if (!csvCell) return;
  generation++; clearPreview(); csvCell.findings = []; element("cell-selections").textContent = "0 selections";
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
