import * as XLSX from "xlsx";
import sax from "sax";
import {xlsxParts} from "./xlsx-package.js";
import {serve, HostError} from "./rpc.js";
import {MAX_CSV_BYTES, MAX_CSV_CELLS} from "./tabular.js";

const main = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const office = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const fail = () => { throw new HostError("invalid_xlsx"); };
const unsupported = () => { throw new HostError("unsupported_xlsx"); };
const attr = (node, key) => node.attributes[key]?.value;
const supportedPart = name => name.endsWith("/") || /^(?:\[Content_Types\]\.xml|_rels\/\.rels|docProps\/(?:app|core|custom)\.xml|xl\/(?:workbook\.xml|styles\.xml|sharedStrings\.xml|_rels\/workbook\.xml\.rels|theme\/[^/]+\.xml|worksheets\/[^/]+\.xml))$/.test(name);

function inspect(parts) {
  const cells = new Map(), sheets = [], relationships = new Map(), formats = new Map(), styles = [];
  let dateSystem = "1900", mainType = false, totalCells = 0, workbookProperties = false;
  for (const [path, bytes] of parts) {
    if (!supportedPart(path)) unsupported();
    if (path.endsWith("/")) continue;
    const parser = sax.parser(true, {xmlns: true, strictEntities: true, position: false});
    const stack = [], sheetCells = new Map();
    let cell, value = "", readingValue = false, root;
    parser.onerror = fail; parser.ondoctype = unsupported;
    parser.onopentag = node => {
      if (stack.length >= 64 || Object.keys(node.attributes).length > 64) throw new HostError("resource_limit");
      if (!stack.length) root = node;
      const parent = stack.at(-1); stack.push(node);
      if (["f", "formula", "formula1", "formula2", "calculatedColumnFormula", "totalsRowFormula"].includes(node.local)) throw new HostError("formulas_not_supported");
      if (["workbookProtection", "sheetProtection"].includes(node.local)) throw new HostError("protected_xlsx");
      if (path === "[Content_Types].xml" && node.local === "Override" && attr(node, "PartName") === "/xl/workbook.xml") {
        mainType = attr(node, "ContentType") === "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
      }
      if (node.local === "Relationship") {
        if (attr(node, "TargetMode") !== undefined && attr(node, "TargetMode") !== "Internal") unsupported();
        const id = attr(node, "Id"), target = attr(node, "Target"), type = attr(node, "Type");
        if (!id || !target || !type) fail();
        if (path === "xl/_rels/workbook.xml.rels") {
          if (relationships.has(id) || !["worksheet", "styles", "sharedStrings", "theme"].some(suffix => type === `${office}/${suffix}`)) unsupported();
          const resolved = target.startsWith("/") ? target.slice(1) : `xl/${target}`;
          if (!parts.has(resolved)) fail();
          relationships.set(id, {target: resolved, type});
        } else if (path === "_rels/.rels" && !["xl/workbook.xml", "docProps/core.xml", "docProps/app.xml", "docProps/custom.xml"].includes(target.replace(/^\//, ""))) unsupported();
      }
      if (path === "xl/workbook.xml" && node.uri === main) {
        if (node.local === "workbookPr") {
          const epoch = attr(node, "date1904");
          if (workbookProperties || (epoch !== undefined && !["0", "1", "false", "true"].includes(epoch))) fail();
          workbookProperties = true; dateSystem = ["1", "true"].includes(epoch) ? "1904" : "1900";
        }
        if (node.local === "sheet") {
          const relationship = Object.values(node.attributes).find(a => a.local === "id" && a.uri === office)?.value;
          const name = attr(node, "name"), visibility = attr(node, "state") ?? "visible";
          if (!name || !relationship || !["visible", "hidden", "veryHidden"].includes(visibility) || sheets.some(sheet => sheet.name === name || sheet.relationship === relationship)) fail();
          sheets.push({name, visibility, relationship});
          if (sheets.length > 32) throw new HostError("resource_limit");
        }
        if (["definedNames", "externalReferences", "pivotCaches", "extLst"].includes(node.local)) unsupported();
      }
      if (path === "xl/styles.xml" && node.uri === main) {
        if (node.local === "numFmt") {
          const id = attr(node, "numFmtId"), format = attr(node, "formatCode");
          if (!id || format === undefined || formats.has(id)) fail(); formats.set(id, format);
        }
        if (node.local === "xf" && parent?.local === "cellXfs") styles.push(attr(node, "numFmtId") ?? "0");
      }
      if (/^xl\/worksheets\//.test(path)) {
        if (node.uri !== main) unsupported();
        if (["drawing", "legacyDrawing", "hyperlinks", "extLst", "tableParts", "conditionalFormatting", "dataValidations", "oleObjects"].includes(node.local)) unsupported();
        if (node.local === "c") {
          const address = attr(node, "r"), type = attr(node, "t") ?? "n", style = attr(node, "s") ?? "0";
          if (cell || !/^[A-Z]{1,3}[1-9]\d{0,6}$/.test(address ?? "") || sheetCells.has(address) || !["n", "s", "inlineStr", "str", "b", "d", "e"].includes(type) || !/^\d+$/.test(style)) fail();
          const decoded = XLSX.utils.decode_cell(address);
          if (decoded.c > 16383 || decoded.r > 1048575) fail();
          cell = {address, type, style: Number(style), value: undefined, payload: undefined};
          if (++totalCells > MAX_CSV_CELLS) throw new HostError("resource_limit");
        } else if (parent?.local === "c" && !["v", "is"].includes(node.local)) unsupported();
        if (["v", "is"].includes(node.local) && cell) {
          if (parent?.local !== "c" || cell.payload !== undefined || ((node.local === "is") !== (cell.type === "inlineStr"))) fail();
          cell.payload = node.local;
          if (node.local === "v") { readingValue = true; value = ""; }
        }
      }
    };
    parser.ontext = text => { if (readingValue) value += text; };
    parser.oncdata = parser.ontext;
    parser.onclosetag = () => {
      const node = stack.pop();
      if (node.local === "v" && readingValue) { cell.value = value; readingValue = false; }
      if (node.local === "c" && cell) { sheetCells.set(cell.address, cell); cell = undefined; }
    };
    parser.write(new TextDecoder("utf-8", {fatal: true}).decode(bytes)).close();
    const expected = path === "xl/workbook.xml" ? "workbook" : /^xl\/worksheets\//.test(path) ? "worksheet" : path === "xl/styles.xml" ? "styleSheet" : undefined;
    if (expected && (root?.local !== expected || root.uri !== main)) unsupported();
    if (/^xl\/worksheets\//.test(path)) cells.set(path, sheetCells);
  }
  if (!mainType || !sheets.length || !parts.has("_rels/.rels")) unsupported();
  const used = new Set();
  for (const sheet of sheets) {
    const relation = relationships.get(sheet.relationship);
    if (!relation || relation.type !== `${office}/worksheet` || !cells.has(relation.target) || used.has(relation.target)) fail();
    sheet.path = relation.target; used.add(sheet.path);
  }
  if (used.size !== cells.size) unsupported();
  const builtins = XLSX.SSF.get_table();
  if (parts.has("xl/styles.xml") && !styles.length) unsupported();
  const numberFormats = (styles.length ? styles : ["0"]).map(id => formats.get(id) ?? builtins[Number(id)]);
  if (numberFormats.some(format => typeof format !== "string")) unsupported();
  return {sheets, cells, dateSystem, numberFormats};
}

function parse(bytes) {
  let parts;
  try {
    if (!(bytes instanceof Uint8Array) || bytes.length > MAX_CSV_BYTES) throw new HostError("resource_limit");
    parts = xlsxParts(bytes);
    const inspected = inspect(parts);
    const book = XLSX.read(bytes, {type: "array", cellFormula: true, cellNF: true, cellText: false, cellHTML: false, cellDates: false, sheetStubs: true, WTF: true});
    if (book.SheetNames.length !== inspected.sheets.length) fail();
    let sourceBytes = 0;
    const sheets = inspected.sheets.map(sheet => {
      if (!book.SheetNames.includes(sheet.name)) fail();
      const parsed = book.Sheets[sheet.name], rawCells = inspected.cells.get(sheet.path), cells = [];
      for (const raw of rawCells.values()) {
        const found = parsed[raw.address];
        if (!found || found.f !== undefined || found.F !== undefined) fail();
        const numberFormat = inspected.numberFormats[raw.style];
        if (numberFormat === undefined) fail();
        let value, sourceType;
        if (["s", "inlineStr", "str"].includes(raw.type)) {
          if (typeof found.v !== "string") fail(); value = found.v; sourceType = "string";
        } else if (raw.value === undefined) { value = ""; sourceType = "blank"; }
        else {
          value = raw.value; sourceType = {n: "number", b: "boolean", d: "date", e: "error"}[raw.type];
          if (raw.type === "n" && (!/^[+-]?(?:\d+\.?\d*|\.\d+)(?:[Ee][+-]?\d+)?$/.test(value) || !Number.isFinite(Number(value)))) fail();
          if (raw.type === "b" && !["0", "1"].includes(value)) fail();
        }
        sourceBytes += new TextEncoder().encode(value).length + new TextEncoder().encode(numberFormat).length;
        if (sourceBytes > MAX_CSV_BYTES) throw new HostError("resource_limit");
        cells.push({address: raw.address, sourceType, value, numberFormat});
      }
      const addresses = Object.keys(parsed).filter(key => !key.startsWith("!"));
      if (addresses.some(address => !rawCells.has(address))) fail();
      sourceBytes += new TextEncoder().encode(sheet.name).length;
      return {name: sheet.name, visibility: sheet.visibility, cells};
    });
    if (sourceBytes > MAX_CSV_BYTES) throw new HostError("resource_limit");
    return {dateSystem: inspected.dateSystem, sheets};
  } catch (error) {
    if (error instanceof HostError) throw error;
    throw new HostError("invalid_xlsx");
  } finally {
    if (bytes instanceof Uint8Array) bytes.fill(0);
    for (const part of parts?.values() ?? []) part.fill(0);
  }
}
serve({parse});
