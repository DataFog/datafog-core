import {pdfFixture} from "./workspace-pdf-fixtures.mjs";
import {workbookFixture} from "./workspace-xlsx-fixtures.mjs";
import {markWorkbookField} from "./test-workspace-xlsx.mjs";

export async function verifyPdfWorker(page) {
  const parsed = await page.evaluate(async bytes => {
    const {client} = await import("./rpc.js");
    const worker = client(new Worker(new URL("./pdf-worker.js", location.href), {type: "module"}));
    try { return await worker.call("parse", [Uint8Array.from(bytes)]); }
    finally { worker.stop(); }
  }, Array.from(pdfFixture()));
  if (parsed.pages.length !== 1 || parsed.pages[0].number !== 1 || !parsed.pages[0].text.includes("Jane Smith") || !parsed.pages[0].text.includes("-12.3400") || !parsed.pages[0].text.includes("00001234")) throw new Error("E1 extracts complete synthetic page text without numeric coercion");
  for (const [variant, code] of [["scanned", "scanned_pdf"], ["mixed", "scanned_pdf"], ["hybrid", "unsupported_pdf"], ["blank", "scanned_pdf"], ["invalid", "invalid_pdf"], ["damagedXref", "invalid_pdf"],
    ["password", "protected_pdf"], ["empty-user-password", "protected_pdf"],
    ["script", "unsupported_pdf"], ["annotation", "unsupported_pdf"], ["form", "unsupported_pdf"], ["attachment", "unsupported_pdf"], ["layers", "unsupported_pdf"], ["invisibleText", "unsupported_pdf"],
    ["pageLimit", "resource_limit"], ["textLimit", "resource_limit"], ["fileLimit", "resource_limit"],
  ]) {
    const result = await page.evaluate(async bytes => {
      const {client} = await import("./rpc.js");
      const worker = client(new Worker(new URL("./pdf-worker.js", location.href), {type: "module"}));
      try { await worker.call("parse", [Uint8Array.from(bytes)]); return {code: "accepted"}; }
      catch (error) { return {code: error.code, message: error.message}; }
      finally { worker.stop(); }
    }, Array.from(pdfFixture(variant)));
    if (result.code !== code || result.message !== code) throw new Error(`E2 ${variant}: expected ${code}, received ${result.code}`);
  }
  const retained = await page.evaluate(async ({bad, good}) => {
    const {importFiles, flattenDocuments} = await import("./documents.js");
    const files = [new File([Uint8Array.from(bad)], "hybrid.pdf"), new File([Uint8Array.from(good)], "text.pdf")];
    const documents = await importFiles(files, [], new AbortController().signal, () => {});
    try { flattenDocuments(documents); return false; }
    catch (error) {
      return error.code === "unsupported_pdf" && error.inputIndex === 0 && documents[0].kind === "rejected"
        && documents[1].kind === "pdf" && flattenDocuments(documents.slice(1)).inputs.length === 2;
    }
  }, {bad: Array.from(pdfFixture("hybrid")), good: Array.from(pdfFixture("pages"))});
  if (!retained) throw new Error("E2 rejected PDFs retain later valid files for explicit removal and review");
  console.log("PASS E1/E2 PDF text extraction, strict rejection and resource limits");
}

async function markPdfPage(page, input, number) {
  await page.getByRole("button", {name: `Review input ${input}`, exact: true}).click();
  await page.locator("#pdf-page").fill(String(number)); await page.locator("#load-cell").click();
  await page.waitForFunction(() => document.querySelector("#status").textContent === "cell_ready");
  await page.locator("#entity-type").fill("PERSON");
  await page.locator("#csv-cell").evaluate(element => {
    const start = element.value.indexOf("Jane Smith"); if (start < 0) throw new Error("synthetic PDF selection missing");
    element.setSelectionRange(start, start + "Jane Smith".length);
  });
  await page.locator("#mark-cell").click();
  await page.waitForFunction(() => document.querySelector("#status").textContent === "selection_added");
}

export async function verifyPdfFlow({fresh, status, markCsvCell, contextClose, launch, unlock, restored, check}) {
  let page = await fresh("pdf");
  await page.locator("#files").setInputFiles([
    {name: "monthly.csv", mimeType: "text/csv", buffer: Buffer.from("Name,Amount\nJane Smith,-10.00")},
    {name: "monthly.xlsx", mimeType: "application/octet-stream", buffer: Buffer.from(workbookFixture())},
    {name: "private-Jane-owner.pdf", mimeType: "application/pdf", buffer: pdfFixture("pages")},
  ]);
  await status(page, "files_added");
  check(!(await page.locator("#inputs").textContent()).includes("unsupported_format"), "E3 PDF intake joins the existing file selection");
  await markCsvCell(page, 1, 2, 1, "Jane Smith");
  for (const [sheet, address, component] of [[1, "A1", "name"], [1, "A1", "value"], [1, "A2", "value"], [2, "A1", "value"], [1, "E3", "numberFormat"]]) await markWorkbookField(page, sheet, address, component, "Jane Smith");
  await markPdfPage(page, 3, 1); await markPdfPage(page, 3, 2);
  await page.locator("#prepare").click(); await status(page, "preview_ready");
  const preview = await page.locator("#preview").textContent();
  const [csv, xlsx, pdf] = preview.replace(/^Input 1\n/, "").split(/\n\nInput \d+\n/).map(text => JSON.parse(text));
  let marker = csv.rows[1][0];
  check(xlsx.sheets[0].name === marker && pdf.pages.every(page => page.text.includes(marker)), "E3 CSV, XLSX and PDF share exact typed aliases");
  check(pdf.format === "pdf" && pdf.pages.length === 2 && pdf.pages[1].number === 2 && pdf.pages[1].rotation === 90 && JSON.stringify(pdf.pages[1].view) === "[0,0,792,612]", "E3 PDF page locations and rotation survive review");
  check(pdf.pages[1].text.includes(`Café ${marker}`) && pdf.pages[0].text.includes("-12.3400") && pdf.pages[0].text.includes("00001234"), "E3 decoded Unicode, numeric text and IDs remain exact");
  check(!preview.includes("Jane Smith") && !preview.includes("private-Jane-owner.pdf"), "E3 original selected names and filenames are absent from outgoing data");
  await page.locator("#clear-cell").click(); await status(page, "selections_cleared");
  check(await page.locator("#approve").isDisabled(), "E3 PDF corrections invalidate approval");
  await markPdfPage(page, 3, 2);
  await page.locator("#files").setInputFiles({name: "scan.pdf", mimeType: "application/pdf", buffer: pdfFixture("scanned")});
  await status(page, "files_added");
  await page.locator("#prepare").click(); await status(page, "scanned_pdf");
  check(await page.locator("#sink").textContent() === "" && await page.locator("#approve").isDisabled(), "E3 rejected PDF blocks the entire mixed batch");
  await page.getByRole("button", {name: "Remove input 4", exact: true}).click(); await status(page, "input_removed");
  await page.locator("#prepare").click(); await status(page, "preview_ready");
  const approved = await page.locator("#preview").textContent();
  const previousMarker = marker;
  marker = JSON.parse(approved.replace(/^Input 1\n/, "").split("\n\nInput 2\n")[0]).rows[1][0];
  check(approved === preview.replaceAll(previousMarker, marker), "E3 fresh preview retains corrected content and shared aliases");
  await page.locator("#approve").click(); await status(page, "saved");
  check(await page.locator("#sink").textContent() === approved && await page.locator("#inputs").textContent() === "" && await page.locator("#csv-cell").inputValue() === "" && await page.locator("#pdf-location").isHidden(), "E4 completion clears PDF source and review state");
  await contextClose(); page = await launch("pdf"); await unlock(page);
  await restored(page, marker, "restored");
  check(await page.locator("#restored").textContent() === "Jane Smith", "E4 PDF alias restores after browser restart");
  await page.locator("#files").setInputFiles({name: "next-month.pdf", mimeType: "application/pdf", buffer: pdfFixture("pages")});
  await status(page, "files_added");
  await markPdfPage(page, 1, 1); await markPdfPage(page, 1, 2);
  await page.locator("#prepare").click(); await status(page, "preview_ready");
  const next = JSON.parse(await page.locator("#preview").textContent());
  check(next.pages.every(page => page.text.includes(marker)), "E4 later PDF uploads reuse saved aliases");
  await page.locator("#cancel").click(); await status(page, "cancelled");
  check(await page.locator("#sink").textContent() === "" && await page.locator("#inputs").textContent() === "" && await page.locator("#csv-cell").inputValue() === "" && await page.locator("#files").inputValue() === "", "E4 cancellation clears temporary PDF data without release");
  await restored(page, marker, "restored");
  check(await page.locator("#restored").textContent() === "Jane Smith", "E4 cancellation preserves committed mappings");
  const cancelled = await page.evaluate(async bytes => {
    const {importFiles} = await import("./documents.js");
    const file = new File([Uint8Array.from(bytes)], "cancel.pdf", {type: "application/pdf"});
    const controller = new AbortController();
    try {
      await importFiles([file], [], controller.signal, (_index, _count, stage) => {
        if (stage === "parsing") queueMicrotask(() => controller.abort());
      });
      return false;
    } catch (error) {
      const original = new Uint8Array(await file.arrayBuffer());
      return error.code === "cancelled" && original.length === bytes.length && original.every((byte, index) => byte === bytes[index]);
    }
  }, Array.from(pdfFixture("pages")));
  check(cancelled, "E4 cancellation terminates a pending PDF worker request and preserves the source File");
  console.log("PASS E3/E4 mixed PDF/CSV/XLSX approval, corrections, cleanup and persistent reuse");
}
