import { client, HostError, safeCode } from "./rpc.js";

export const MAX_SOURCE_BYTES = 1024 * 1024;
export const MAX_TEXT_LEAVES = 100000;
export const MAX_FILES = 32;
const encoder = new TextEncoder();

/** Textareas normalize CRLF/CR to LF. Map their UTF-16 selection back before Core conversion. */
export function cellSelection(text, start, end) {
  let display = 0, originalStart, originalEnd;
  for (let source = 0; source <= text.length; source++) {
    if (display === start) originalStart = source;
    if (display === end) originalEnd = source;
    if (source === text.length) break;
    if (text[source] === "\r" && text[source + 1] === "\n") source++;
    display++;
  }
  if (originalStart === undefined || originalEnd === undefined || start > end) throw new HostError("invalid_finding");
  return {start: originalStart, end: originalEnd};
}

function document(rows) {
  const fields = [], widths = [], offsets = [];
  let sourceBytes = 0;
  for (const row of rows) {
    offsets.push(fields.length); widths.push(row.length);
    for (const text of row) {
      sourceBytes += encoder.encode(text).length;
      fields.push({text, findings: []});
    }
  }
  return {kind: "csv", fields, widths, offsets, sourceBytes};
}

function pdfDocument(document) {
  const fields = document.pages.map(page => ({text: page.text, findings: []}));
  const pages = document.pages.map(({number, view, rotation, userUnit}, fieldIndex) => ({number, view, rotation, userUnit, fieldIndex}));
  return {kind: "pdf", fields, pages, sourceBytes: fields.reduce((sum, field) => sum + encoder.encode(field.text).length, 0)};
}

function readBytes(file, signal) {
  return new Promise((resolve, reject) => {
    if (signal.aborted) { reject(new HostError("cancelled")); return; }
    const reader = new FileReader();
    const abort = () => reader.abort();
    const finish = (error, bytes) => {
      signal.removeEventListener("abort", abort);
      reader.onload = reader.onerror = reader.onabort = null;
      if (error) reject(new HostError(error)); else resolve(bytes);
    };
    reader.onload = () => finish(undefined, new Uint8Array(reader.result));
    reader.onerror = () => finish("file_read_failed");
    reader.onabort = () => finish("cancelled");
    signal.addEventListener("abort", abort, {once: true});
    reader.readAsArrayBuffer(file);
  });
}

/** All selected files produce either a document or a blocking rejection. */
export async function importFiles(files, existing, signal, progress) {
  if (files.length + existing.length > MAX_FILES) return [{kind: "rejected", error: "resource_limit"}];
  const workers = new Map();
  const workerFor = format => {
    if (!workers.has(format)) workers.set(format, client(new Worker(new URL(`./${format}-worker.js`, import.meta.url), {type: "module"})));
    return workers.get(format);
  };
  const abort = () => { for (const worker of workers.values()) worker.stop(); };
  signal.addEventListener("abort", abort, {once: true});
  let sourceBytes = existing.reduce((sum, input) => sum + (input.sourceBytes ?? encoder.encode(input.text ?? "").length), 0);
  let fields = existing.reduce((sum, input) => sum + (input.fields?.length ?? 1), 0);
  const imported = [];
  try {
    for (const [index, file] of files.entries()) {
      if (signal.aborted) throw new HostError("cancelled");
      progress(index + 1, files.length);
      let bytes, timer;
      try {
        const format = /\.(csv|pdf)$/i.exec(file.name)?.[1].toLowerCase();
        if (!format) throw new HostError("unsupported_format");
        if (file.size > MAX_SOURCE_BYTES) throw new HostError("resource_limit");
        bytes = await readBytes(file, signal);
        if (signal.aborted) throw new HostError("cancelled");
        progress(index + 1, files.length, "parsing");
        if (signal.aborted) throw new HostError("cancelled");
        const worker = workerFor(format), parsing = worker.call("parse", [bytes]);
        const parsed = format === "pdf" ? await Promise.race([parsing, new Promise((_, reject) => {
          timer = setTimeout(() => { reject(new HostError("resource_limit")); worker.stop(); workers.delete(format); }, 30000);
        })]) : await parsing;
        if (signal.aborted) throw new HostError("cancelled");
        const input = format === "csv" ? document(parsed) : pdfDocument(parsed);
        if (sourceBytes + input.sourceBytes > MAX_SOURCE_BYTES || fields + input.fields.length > MAX_TEXT_LEAVES) throw new HostError("resource_limit");
        sourceBytes += input.sourceBytes; fields += input.fields.length;
        imported.push({...input, label: file.name});
      } catch (error) {
        if (signal.aborted || error.code === "cancelled") throw new HostError("cancelled");
        imported.push({kind: "rejected", error: safeCode(error), label: file.name});
      } finally { clearTimeout(timer); bytes?.fill(0); }
    }
    return imported;
  } finally {
    signal.removeEventListener("abort", abort); abort();
  }
}

/** Core sees text leaves; file, record and page structure stays in the host. */
export function flattenDocuments(documents) {
  if (documents.length > MAX_FILES) throw new HostError("resource_limit");
  const inputs = [], locations = [];
  for (const [inputIndex, input] of documents.entries()) {
    if (input.kind === "rejected") throw new HostError(input.error, {inputIndex});
    if (input.kind === "pdf") {
      const base = inputs.length; for (const field of input.fields) inputs.push(field);
      for (const [pageIndex, page] of input.pages.entries()) locations[base + page.fieldIndex] = {inputIndex, pageIndex};
    } else if (input.kind !== "csv") {
      inputs.push({text: input.text, findings: input.findings}); locations.push({inputIndex});
    } else {
      input.widths.forEach((width, rowIndex) => {
        for (let columnIndex = 0; columnIndex < width; columnIndex++) {
          inputs.push(input.fields[input.offsets[rowIndex] + columnIndex]);
          locations.push({inputIndex, rowIndex, columnIndex});
        }
      });
    }
  }
  return {inputs, locations};
}

export function formatDocuments(documents, texts) {
  let cursor = 0;
  const result = documents.map(input => {
    if (input.kind === "pdf") {
      const base = cursor; cursor += input.fields.length;
      return JSON.stringify({format: "pdf", pages: input.pages.map(({number, view, rotation, userUnit, fieldIndex}) => ({number, view, rotation, userUnit, text: texts[base + fieldIndex]}))});
    }
    if (input.kind !== "csv") return texts[cursor++];
    const rows = input.widths.map(width => {
      const row = texts.slice(cursor, cursor + width); cursor += width; return row;
    });
    return JSON.stringify({format: "csv", rows});
  });
  if (cursor !== texts.length || result.some(text => typeof text !== "string")) throw new HostError("invalid_batch");
  const formatted = result.length === 1 ? result[0] : result.map((text, i) => `Input ${i + 1}\n${text}`).join("\n\n");
  if (encoder.encode(formatted).length > 16 * 1024 * 1024) throw new HostError("resource_limit");
  return formatted;
}
