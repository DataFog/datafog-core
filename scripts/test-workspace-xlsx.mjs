/** Installed extension-worker checks, called by the shared browser integration run. */
import {workbookFixture} from "./workspace-xlsx-fixtures.mjs";

export async function verifyXlsxWorker(page) {
  const parsed = await page.evaluate(async bytes => {
    const {client} = await import("./rpc.js");
    const worker = client(new Worker(new URL("./xlsx-worker.js", location.href), {type: "module"}));
    try { return await worker.call("parse", [Uint8Array.from(bytes)]); }
    finally { worker.stop(); }
  }, Array.from(workbookFixture()));
  if (parsed.sheets.length !== 2 || parsed.sheets[0].name !== "Jane Smith" || parsed.sheets[1].visibility !== "hidden") throw new Error("D1 sheets and names are retained");
  const cells = parsed.sheets[0].cells;
  const cell = address => cells.find(c => c.address === address);
  if (cell("A1").value !== "Jane Smith" || cell("B1").value !== "1234" || cell("B1").numberFormat !== "00000000") throw new Error("D1 text and leading-zero number formats are retained");
  if (cell("C1").value !== "12.3400" || cell("C2").value !== "9007199254740993" || cell("D1").value !== "46317" || parsed.dateSystem !== "1900") throw new Error("D1 numeric lexemes and date system remain exact");
  if (cell("XFD1048576").value !== "outside declared range" || cell("D2").value !== "2026-10-22T10:15:30") throw new Error("D1 source coordinates and date cells are preserved");
  for (const [variant, code] of [
    ["formula", "formulas_not_supported"], ["sharedFormula", "formulas_not_supported"],
    ["arrayFormula", "formulas_not_supported"], ["prefixedFormula", "formulas_not_supported"],
    ["orphanFormula", "formulas_not_supported"], ["duplicateCell", "invalid_xlsx"],
    ["malformedXml", "invalid_xlsx"], ["doctype", "unsupported_xlsx"],
    ["unsupportedPart", "unsupported_xlsx"], ["macro", "unsupported_xlsx"],
    ["external", "unsupported_xlsx"], ["missingSheet", "invalid_xlsx"],
    ["badStyle", "invalid_xlsx"], ["bomb", "resource_limit"],
    ["traversal", "invalid_xlsx"], ["truncated", "invalid_xlsx"],
    ["badCrc", "invalid_xlsx"], ["forgedSize", "invalid_xlsx"], ["encrypted", "protected_xlsx"],
    ["badEpoch", "invalid_xlsx"], ["ambiguousCell", "invalid_xlsx"],
    ["sheetPassword", "protected_xlsx"], ["repeatedStrings", "resource_limit"],
  ]) {
    const result = await page.evaluate(async bytes => {
      const {client} = await import("./rpc.js");
      const worker = client(new Worker(new URL("./xlsx-worker.js", location.href), {type: "module"}));
      try { await worker.call("parse", [Uint8Array.from(bytes)]); return {code: "accepted"}; }
      catch (error) { return {code: error.code, message: error.message}; }
      finally { worker.stop(); }
    }, Array.from(workbookFixture(variant)));
    if (result.code !== code || result.message !== code) throw new Error(`D2 ${variant}: expected ${code}, received ${result.code}`);
  }
  const epoch = await page.evaluate(async bytes => {
    const {client} = await import("./rpc.js");
    const worker = client(new Worker(new URL("./xlsx-worker.js", location.href), {type: "module"}));
    try { return (await worker.call("parse", [Uint8Array.from(bytes)])).dateSystem; }
    finally { worker.stop(); }
  }, Array.from(workbookFixture("date1904")));
  if (epoch !== "1904") throw new Error("D1 1904 date system is preserved");
  for (const [variant, values] of [["literalFormula", ["=SUM(1,2)"]], ["sharedStrings", [" Jane Smith "]], ["emptyCells", ["", "", "#N/A"]]]) {
    const actual = await page.evaluate(async bytes => {
      const {client} = await import("./rpc.js");
      const worker = client(new Worker(new URL("./xlsx-worker.js", location.href), {type: "module"}));
      try { return (await worker.call("parse", [Uint8Array.from(bytes)])).sheets[1].cells.map(cell => cell.value); }
      finally { worker.stop(); }
    }, Array.from(workbookFixture(variant)));
    if (JSON.stringify(actual) !== JSON.stringify(values)) throw new Error(`D1 ${variant} retains source text`);
  }
  console.log("PASS D1/D2 workbook source values, all-sheet formula rejection and package validation");
}

async function markWorkbookField(page, sheet, address, component, value, entityType = "PERSON") {
  await page.getByRole("button", {name: "Review input 2", exact: true}).click();
  await page.locator("#xlsx-sheet").fill(String(sheet)); await page.locator("#xlsx-address").fill(address);
  await page.locator("#xlsx-component").selectOption(component);
  await page.locator("#load-cell").click();
  await page.waitForFunction(() => document.querySelector("#status").textContent === "cell_ready");
  await page.locator("#entity-type").fill(entityType);
  await page.locator("#csv-cell").evaluate((element, value) => {
    const start = element.value.indexOf(value); if (start < 0) throw new Error("synthetic workbook selection missing");
    element.setSelectionRange(start, start + value.length);
  }, value);
  await page.locator("#mark-cell").click();
  await page.waitForFunction(() => document.querySelector("#status").textContent === "selection_added");
}

export async function verifyXlsxFlow({fresh, status, markCsvCell, contextClose, launch, unlock, restored, check}) {
  let page = await fresh("xlsx");
  await page.locator("#files").setInputFiles([
    {name: "monthly.csv", mimeType: "text/csv", buffer: Buffer.from("Name,Amount\nJane Smith,-10.00")},
    {name: "private-Jane-owner.xlsx", mimeType: "application/octet-stream", buffer: Buffer.from(workbookFixture())},
  ]);
  await status(page, "files_added");
  await markCsvCell(page, 1, 2, 1, "Jane Smith");
  await markWorkbookField(page, 1, "A1", "name", "Jane Smith");
  await markWorkbookField(page, 1, "A1", "value", "Jane Smith");
  await markWorkbookField(page, 1, "A2", "value", "Jane Smith");
  await markWorkbookField(page, 2, "A1", "value", "Jane Smith");
  await markWorkbookField(page, 1, "E3", "numberFormat", "Jane Smith");
  await page.locator("#prepare").click(); await status(page, "preview_ready");
  const preview = await page.locator("#preview").textContent();
  const [csvText, xlsxText] = preview.split("\n\nInput 2\n");
  const csv = JSON.parse(csvText.replace(/^Input 1\n/, "")), xlsx = JSON.parse(xlsxText);
  const marker = csv.rows[1][0], cell = address => xlsx.sheets[0].cells.find(c => c.address === address);
  check(xlsx.sheets[0].name === marker && cell("A1").value === marker && xlsx.sheets[1].cells[0].value === marker, "D3 CSV, sheet names and hidden-sheet cells share exact pseudonyms");
  check(cell("A2").value === `👋 é ${marker}` && cell("E3").numberFormat === `"${marker} "0`, "D3 Unicode and sensitive number-format literals are reviewed through Core");
  check(cell("B1").value === "1234" && cell("B1").numberFormat === "00000000" && cell("C1").value === "12.3400" && cell("C2").value === "9007199254740993" && xlsx.dateSystem === "1900", "D3 exact source values and formats survive approval preparation");
  check(!preview.includes("Jane Smith") && !preview.includes("private-Jane-owner.xlsx"), "D3 raw selected values and filenames are absent from outgoing data");
  await page.locator("#files").setInputFiles({name: "formula.xlsx", mimeType: "application/octet-stream", buffer: Buffer.from(workbookFixture("sharedFormula"))});
  await status(page, "files_added");
  check(await page.locator("#approve").isDisabled(), "D3 new workbook invalidates approval");
  await page.locator("#prepare").click(); await status(page, "formulas_not_supported");
  check((await page.locator("#batch-error").textContent()).includes("values-only"), "D3 formula rejection gives values-only/CSV guidance");
  check(await page.locator("#sink").textContent() === "", "D3 formula workbook blocks the complete mixed batch");
  await page.getByRole("button", {name: "Remove input 3", exact: true}).click(); await status(page, "input_removed");
  await page.locator("#prepare").click(); await status(page, "preview_ready");
  const approved = await page.locator("#preview").textContent();
  const savedMarker = approved.match(/\[\[DFP1:[^\]]+:PERSON:1\]\]/)[0];
  await page.locator("#approve").click(); await status(page, "saved");
  check(await page.locator("#sink").textContent() === approved && await page.locator("#csv-cell").inputValue() === "" && await page.locator("#inputs").textContent() === "", "D4 saved workbook drops temporary review data");
  await contextClose(); page = await launch("xlsx"); await unlock(page);
  await restored(page, savedMarker, "restored");
  check(await page.locator("#restored").textContent() === "Jane Smith", "D4 workbook pseudonyms restore after browser restart");
  await page.locator("#files").setInputFiles({name: "next.xlsx", mimeType: "application/octet-stream", buffer: Buffer.from(workbookFixture())});
  await status(page, "files_added");
  await page.locator("#cancel").click(); await status(page, "cancelled");
  check(await page.locator("#files").inputValue() === "" && await page.locator("#inputs").textContent() === "" && await page.locator("#xlsx-location").isHidden(), "D4 cancelled workbook clears values, formats and names");
  console.log("PASS D3/D4 mixed CSV/XLSX review, formula blocking, approval and restart");
}
