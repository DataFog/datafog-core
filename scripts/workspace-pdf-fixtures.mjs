/** Public synthetic PDF objects; offsets and stream lengths are generated exactly. */
import {readFileSync} from "node:fs";
import {deflateSync} from "node:zlib";

export function pdfFixture(variant = "text") {
  if (["password", "empty-user-password"].includes(variant)) return readFileSync(new URL(`../fixtures/workspace/pdf/${variant}.pdf`, import.meta.url));
  if (variant === "fileLimit") return Buffer.alloc(1024 * 1024 + 1);
  const stream = text => `<< /Length ${Buffer.byteLength(text, "latin1")} >>\nstream\n${text}\nendstream`;
  const contents = "BT /F1 12 Tf 50 740 Td (Jane ) Tj (Smith) Tj 0 -24 Td (2026-10-22  Grocery  -12.3400) Tj 0 -24 Td (Account 00001234) Tj ET";
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    stream(contents),
  ];
  const image = "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\n\xff\nendstream";
  const imageContent = "q 512 0 0 512 50 100 cm /Im1 Do Q";
  if (variant === "scanned" || variant === "hybrid") {
    objects[2] = objects[2].replace("/Font <<", "/XObject << /Im1 6 0 R >> /Font <<");
    objects[4] = stream((variant === "hybrid" ? contents + "\n" : "") + imageContent); objects.push(image);
  }
  if (variant === "pages" || variant === "mixed") {
    objects[1] = "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>";
    objects.push(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 792 612] /Rotate 90 /Resources << /Font << /F1 4 0 R >> ${variant === "mixed" ? "/XObject << /Im1 8 0 R >>" : ""} >> /Contents 7 0 R >>`);
    objects.push(stream(variant === "mixed" ? imageContent : "BT /F1 12 Tf 40 500 Td (Caf\xe9 Jane Smith) Tj 0 -24 Td (Page 2  00123  +45.6700) Tj ET"));
    if (variant === "mixed") objects.push(image);
  }
  if (variant === "blank") objects[4] = stream("");
  if (variant === "script") {
    objects[0] = "<< /Type /Catalog /Pages 2 0 R /OpenAction 6 0 R >>";
    objects.push("<< /S /JavaScript /JS (app.alert\\(\"synthetic-only\"\\)) >>");
  }
  if (variant === "annotation" || variant === "form") {
    objects[2] = objects[2].replace("/Contents", "/Annots [6 0 R] /Contents");
    objects.push(variant === "annotation" ? "<< /Type /Annot /Subtype /Text /Rect [10 10 50 50] /Contents (Jane Smith) >>" : "<< /Type /Annot /Subtype /Widget /FT /Tx /T (Owner) /V (Jane Smith) /Rect [10 10 50 50] /P 3 0 R >>");
    if (variant === "form") objects[0] = objects[0].replace("/Pages", "/AcroForm << /Fields [6 0 R] >> /Pages");
  }
  if (variant === "layers") {
    objects[0] = objects[0].replace("/Pages", "/OCProperties << /OCGs [6 0 R] /D << /Order [6 0 R] /ON [6 0 R] >> >> /Pages");
    objects.push("<< /Type /OCG /Name (Synthetic layer) >>");
  }
  if (variant === "attachment") {
    objects[0] = objects[0].replace("/Pages", "/Names << /EmbeddedFiles << /Names [(private.txt) 6 0 R] >> >> /Pages");
    objects.push("<< /Type /Filespec /F (private.txt) /EF << /F 7 0 R >> >>", stream("Jane Smith"));
  }
  if (variant === "invisibleText") objects[4] = stream(contents.replace("12 Tf", "12 Tf 3 Tr"));
  if (variant === "pageLimit" || variant === "textLimit") {
    const count = variant === "pageLimit" ? 257 : 17;
    const kids = [3];
    for (let index = 1; index < count; index++) { kids.push(objects.length + 1); objects.push(objects[2]); }
    objects[1] = `<< /Type /Pages /Kids [${kids.map(id => `${id} 0 R`).join(" ")}] /Count ${count} >>`;
    if (variant === "textLimit") {
      const compressed = deflateSync(Buffer.from(`BT /F1 0.001 Tf 0 400 Td (${"x".repeat(70000)}) Tj ET`));
      objects[4] = `<< /Length ${compressed.length} /Filter /FlateDecode >>\nstream\n${compressed.toString("latin1")}\nendstream`;
    }
  }
  let result = "%PDF-1.7\n%\xE2\xE3\xCF\xD3\n";
  const offsets = [0];
  for (const [index, object] of objects.entries()) {
    offsets.push(Buffer.byteLength(result, "latin1")); result += `${index + 1} 0 obj\n${object}\nendobj\n`;
  }
  const start = Buffer.byteLength(result, "latin1");
  result += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n${offsets.slice(1).map(offset => `${String(offset).padStart(10, "0")} 00000 n \n`).join("")}trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${start}\n%%EOF\n`;
  if (variant === "invalid") return Buffer.from("not a PDF");
  if (variant === "damagedXref") result = result.replace(/startxref\n\d+/, "startxref\n0");
  return Buffer.from(result, "latin1");
}
