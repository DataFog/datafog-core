import {getDocument, PDFWorker, OPS, AnnotationMode} from "pdfjs-dist/legacy/build/pdf.mjs";
import "pdfjs-dist/legacy/build/pdf.worker.mjs";
import {serve, HostError} from "./rpc.js";
import {MAX_SOURCE_BYTES} from "./documents.js";

// PDF.js can include source excerpts in diagnostics. Record only their occurrence
// inside this dedicated worker and reject source-time warnings instead of logging.
let diagnostic = false;
for (const method of ["log", "warn", "error", "info", "debug"]) console[method] = () => { diagnostic = true; };
const encoder = new TextEncoder();
const imageOperators = new Set(Object.entries(OPS).filter(([name]) => name.includes("Image")).map(([, value]) => value));

async function parse(bytes) {
  let engine, task;
  try {
    if (!(bytes instanceof Uint8Array) || bytes.length > MAX_SOURCE_BYTES) throw new HostError("resource_limit");
    if (new TextDecoder().decode(bytes.subarray(0, 5)) !== "%PDF-") throw new HostError("invalid_pdf");
    // The prebundled parser runs in this terminable worker. Finish its loopback
    // startup before admitting source data; no document has been opened yet.
    engine = new PDFWorker({verbosity: 1}); await engine.promise; diagnostic = false;
    // PDF.js 6.4 can resolve getOperatorList with partial data after a stream
    // error. Observe its error replies too; retain no exception/source text.
    let parserFailure = false;
    engine.port.addEventListener("message", ({data}) => { if (data && Object.hasOwn(data, "reason")) parserFailure = true; });
    task = getDocument({data: bytes, worker: engine, verbosity: 1, stopAtErrors: true,
      disableFontFace: true, useSystemFonts: false, useWasm: false, useWorkerFetch: true,
      standardFontDataUrl: new URL("./pdf-resources/standard_fonts/", import.meta.url).href,
      cMapUrl: new URL("./pdf-resources/cmaps/", import.meta.url).href, cMapPacked: true,
      isOffscreenCanvasSupported: false, isImageDecoderSupported: false,
      maxImageSize: 0,
    });
    const document = await task.promise;
    if (document.numPages < 1 || document.numPages > 256) throw new HostError("resource_limit");
    const {info} = await document.getMetadata();
    if (info.EncryptFilterName != null || await document.getPermissions() !== null) throw new HostError("protected_pdf");
    if (document.isPureXfa || info.IsAcroFormPresent || info.IsXFAPresent || info.IsCollectionPresent || info.IsSignaturesPresent
      || await document.getAttachments() || await document.getJSActions() || (await document.getOptionalContentConfig()).getOrder() !== null) throw new HostError("unsupported_pdf");
    const pages = [];
    let sourceBytes = 0, items = 0;
    for (let number = 1; number <= document.numPages; number++) {
      const page = await document.getPage(number);
      if ((await page.getAnnotations({intent: "any"})).length || await page.getJSActions()) throw new HostError("unsupported_pdf");
      let text = "", rejection;
      for await (const chunk of page.streamTextContent({disableNormalization: true})) {
        for (const item of chunk.items) {
          // Drain this page without retaining further text after a limit. Early
          // iterator cancellation races PDF.js's stream close; host cancellation
          // and the file deadline terminate the entire dedicated worker instead.
          if (rejection) continue;
          if (typeof item.str !== "string") { rejection = "unsupported_pdf"; continue; }
          const part = item.str + (item.hasEOL ? "\n" : "");
          if (part.length > MAX_SOURCE_BYTES) { rejection = "resource_limit"; continue; }
          sourceBytes += encoder.encode(part).length;
          if (sourceBytes > MAX_SOURCE_BYTES || ++items > 100000) { rejection = "resource_limit"; continue; }
          text += part;
        }
      }
      if (rejection) throw new HostError(rejection);
      if (!text.trim()) throw new HostError("scanned_pdf");
      try {
        const operations = await page.getOperatorList({intent: "any", annotationMode: AnnotationMode.DISABLE});
        if (parserFailure) throw new HostError("unsupported_pdf");
        if (operations.fnArray.length > 100000) throw new HostError("resource_limit");
        if (operations.fnArray.some((op, index) => imageOperators.has(op) || (op === OPS.setTextRenderingMode && [3, 7].includes(operations.argsArray[index][0])))) throw new HostError("unsupported_pdf");
      } catch (error) {
        if (error instanceof HostError) throw error;
        throw new HostError("unsupported_pdf");
      }
      if (page.view.length !== 4 || !page.view.every(Number.isFinite) || !Number.isFinite(page.userUnit) || page.userUnit <= 0 || !Number.isInteger(page.rotate) || page.rotate % 90 !== 0) throw new HostError("invalid_pdf");
      pages.push({number, view: page.view, rotation: page.rotate, userUnit: page.userUnit, text});
      if (!page.cleanup()) throw new HostError("invalid_pdf");
    }
    if (diagnostic) throw new HostError("invalid_pdf");
    return {pages};
  } catch (error) {
    if (error instanceof HostError) throw error;
    throw new HostError(error?.name === "PasswordException" ? "protected_pdf" : "invalid_pdf");
  } finally {
    try { await task?.destroy(); } finally { engine?.destroy(); if (bytes instanceof Uint8Array && bytes.byteLength) bytes.fill(0); }
  }
}
serve({parse});
