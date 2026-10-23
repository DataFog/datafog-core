/** Minimal public OOXML fixtures, including deliberately malformed packages. */
import {createRequire} from "node:module";
const {zipSync, strToU8} = createRequire(new URL("../examples/workspace-extension/package.json", import.meta.url))("fflate");
const main = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const office = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const packageNs = "http://schemas.openxmlformats.org/package/2006/relationships";
const xml = body => `<?xml version="1.0" encoding="UTF-8"?>${body}`;

export function workbookParts(variant = "values") {
  const parts = {
    "[Content_Types].xml": xml('<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>'),
    "_rels/.rels": xml(`<Relationships xmlns="${packageNs}"><Relationship Id="root" Type="${office}/officeDocument" Target="xl/workbook.xml"/></Relationships>`),
    "xl/workbook.xml": xml(`<workbook xmlns="${main}" xmlns:r="${office}"><workbookPr date1904="0"/><sheets><sheet name="Jane Smith" sheetId="1" r:id="one"/><sheet name="Hidden statements" sheetId="2" state="hidden" r:id="two"/></sheets></workbook>`),
    "xl/_rels/workbook.xml.rels": xml(`<Relationships xmlns="${packageNs}"><Relationship Id="one" Type="${office}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="two" Type="${office}/worksheet" Target="worksheets/sheet2.xml"/><Relationship Id="style" Type="${office}/styles" Target="styles.xml"/></Relationships>`),
    "xl/styles.xml": xml(`<styleSheet xmlns="${main}"><numFmts count="2"><numFmt numFmtId="164" formatCode="00000000"/><numFmt numFmtId="165" formatCode="0.0000"/></numFmts><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf numFmtId="0"/></cellStyleXfs><cellXfs count="4"><xf numFmtId="0"/><xf numFmtId="164"/><xf numFmtId="165"/><xf numFmtId="14"/></cellXfs></styleSheet>`),
    "xl/worksheets/sheet1.xml": xml(`<worksheet xmlns="${main}"><dimension ref="A1:D2"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Jane Smith</t></is></c><c r="B1" s="1"><v>1234</v></c><c r="C1" s="2"><v>12.3400</v></c><c r="D1" s="3"><v>46317</v></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>👋 é Jane Smith</t></is></c><c r="B2" t="b"><v>1</v></c><c r="C2"><v>9007199254740993</v></c><c r="D2" t="d"><v>2026-10-22T10:15:30</v></c></row><row r="1048576"><c r="XFD1048576" t="inlineStr"><is><t>outside declared range</t></is></c></row></sheetData></worksheet>`),
    "xl/worksheets/sheet2.xml": xml(`<worksheet xmlns="${main}"><sheetData><row r="1" hidden="1"><c r="A1" t="inlineStr"><is><t>Jane Smith</t></is></c></row></sheetData></worksheet>`),
  };
  parts["xl/styles.xml"] = parts["xl/styles.xml"].replace('numFmts count="2"', 'numFmts count="3"').replace('</numFmts>', '<numFmt numFmtId="166" formatCode="&quot;Jane Smith &quot;0"/></numFmts>').replace('cellXfs count="4"', 'cellXfs count="5"').replace('</cellXfs>', '<xf numFmtId="166"/></cellXfs>');
  parts["xl/worksheets/sheet1.xml"] = parts["xl/worksheets/sheet1.xml"].replace('<row r="1048576">', '<row r="3"><c r="E3" s="4"><v>99</v></c></row><row r="1048576">');
  const sheet = "xl/worksheets/sheet2.xml";
  if (variant === "formula" || variant === "sharedFormula" || variant === "arrayFormula" || variant === "prefixedFormula") {
    const formula = variant === "sharedFormula" ? '<f t="shared" si="0"/>' : variant === "arrayFormula" ? '<f t="array" ref="A1:B2">SUM(1,2)</f>' : variant === "prefixedFormula" ? `<s:f xmlns:s="${main}">SUM(1,2)</s:f>` : "<f>SUM(1,2)</f>";
    parts[sheet] = xml(`<worksheet xmlns="${main}"><dimension ref="A1"/><sheetData><row r="1048576"><c r="XFD1048576">${formula}<v>3</v></c></row></sheetData></worksheet>`);
  }
  if (variant === "date1904") parts["xl/workbook.xml"] = parts["xl/workbook.xml"].replace('date1904="0"', 'date1904="1"');
  if (variant === "literalFormula") parts[sheet] = parts[sheet].replace("Jane Smith", "=SUM(1,2)");
  if (variant === "sharedStrings" || variant === "repeatedStrings") {
    const text = variant === "sharedStrings" ? '<r><t xml:space="preserve"> Jane </t></r><r><t>Smith </t></r>' : `<t>${"x".repeat(32000)}</t>`;
    parts["xl/sharedStrings.xml"] = xml(`<sst xmlns="${main}"><si>${text}</si></sst>`);
    parts["[Content_Types].xml"] = parts["[Content_Types].xml"].replace("</Types>", '<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>');
    parts["xl/_rels/workbook.xml.rels"] = parts["xl/_rels/workbook.xml.rels"].replace("</Relationships>", `<Relationship Id="strings" Type="${office}/sharedStrings" Target="sharedStrings.xml"/></Relationships>`);
    const cells = Array.from({length: variant === "sharedStrings" ? 1 : 40}, (_, index) => `<row r="${index + 1}"><c r="A${index + 1}" t="s"><v>0</v></c></row>`).join("");
    parts[sheet] = xml(`<worksheet xmlns="${main}"><sheetData>${cells}</sheetData></worksheet>`);
  }
  if (variant === "emptyCells") parts[sheet] = xml(`<worksheet xmlns="${main}"><sheetData><row r="1"><c r="A1" s="1"/><c r="B1" t="inlineStr"><is><t></t></is></c><c r="C1" t="e"><v>#N/A</v></c></row></sheetData></worksheet>`);
  if (variant === "badEpoch") parts["xl/workbook.xml"] = parts["xl/workbook.xml"].replace('date1904="0"', 'date1904="invalid"');
  if (variant === "ambiguousCell") parts[sheet] = parts[sheet].replace("</is>", "</is><v>42</v>");
  if (variant === "sheetPassword") parts[sheet] = parts[sheet].replace("</worksheet>", '<sheetProtection sheet="1" password="CAFE"/></worksheet>');
  if (variant === "duplicateCell") parts[sheet] = parts[sheet].replace("</row>", '<c r="A1"><v>42</v></c></row>');
  if (variant === "malformedXml") parts[sheet] = parts[sheet].replace("</row>", "</wrong>");
  if (variant === "doctype") parts[sheet] = parts[sheet].replace("<worksheet", '<!DOCTYPE worksheet [<!ENTITY private "private-value">]><worksheet');
  if (variant === "unsupportedPart") parts["xl/drawings/drawing1.xml"] = xml("<drawing/>");
  if (variant === "macro") parts["[Content_Types].xml"] = parts["[Content_Types].xml"].replace("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml", "application/vnd.ms-excel.sheet.macroEnabled.main+xml");
  if (variant === "external") parts["xl/_rels/workbook.xml.rels"] = parts["xl/_rels/workbook.xml.rels"].replace('Target="worksheets/sheet2.xml"', 'Target="https://example.test/private" TargetMode="External"');
  if (variant === "missingSheet") delete parts[sheet];
  if (variant === "orphanFormula") parts["xl/worksheets/sheet3.xml"] = xml(`<worksheet xmlns="${main}"><sheetData><row r="1"><c r="A1"><f>1+1</f><v>2</v></c></row></sheetData></worksheet>`);
  if (variant === "badStyle") parts[sheet] = parts[sheet].replace('r="A1"', 'r="A1" s="99"');
  if (variant === "bomb") parts["docProps/custom.xml"] = " ".repeat(16 * 1024 * 1024 + 1);
  if (variant === "traversal") parts["../private.xml"] = xml("<private/>");
  return parts;
}

export function workbookFixture(variant = "values") {
  const bytes = zipSync(Object.fromEntries(Object.entries(workbookParts(variant)).map(([name, text]) => [name, strToU8(text)])), {level: 6});
  if (variant === "truncated") return bytes.slice(0, -7);
  if (variant === "badCrc" || variant === "forgedSize") {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const directory = view.getUint32(bytes.length - 6, true);
    if (variant === "badCrc") { bytes[14] ^= 1; bytes[directory + 16] ^= 1; }
    else {
      const size = view.getUint32(22, true) - 1;
      view.setUint32(22, size, true); view.setUint32(directory + 24, size, true);
    }
  }
  if (variant === "encrypted") { bytes[6] |= 1; }
  return bytes;
}
