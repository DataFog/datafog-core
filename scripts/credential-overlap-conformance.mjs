import { expandSyntheticFixture } from "./synthetic-fixtures.mjs";
// Shared installed-package checks, executed in Node and in a real browser.
export function verifyCredentialOverlaps(api, records) {
  const equal = (actual, expected, message) => {
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      throw new Error(`${message}: ${JSON.stringify(actual)} != ${JSON.stringify(expected)}`);
    }
  };
  function project(text, findings) {
    return findings.filter(f => ["API_KEY", "BEARER_TOKEN", "CREDENTIAL_URI", "JWT"].includes(f.entityType)).map(f => {
      const bytes = new TextEncoder().encode(text);
      equal(new TextDecoder().decode(bytes.slice(f.byteRange.start, f.byteRange.end)), f.matchedText, "bytes");
      equal(text.slice(f.utf16Range.start, f.utf16Range.end), f.matchedText, "UTF-16");
      equal(Array.from(text).slice(f.codepointRange.start, f.codepointRange.end).join(""), f.matchedText, "code points");
      equal(f.confidence, undefined, "confidence");
      equal(f.detectorName, "datafog-core/" + f.entityType.toLowerCase().replaceAll("_", "-"), "detector");
      if (!f.detectorVersion) throw new Error("missing version");
      return {label:f.entityType, text:f.matchedText, start:f.codepointRange.start, end:f.codepointRange.end};
    });
  }
  for (const row of records.map(expandSyntheticFixture)) {
    const findings = api.scan(row.text, row.config);
    if (row.authorization_value) {
      const value = row.authorization_value;
      const data = {"👋/headers": {aUtHoRiZaTiOn: value}};
      const located = api.scanStructured(data).findings;
      if (located.some(item => item.path !== "/👋~1headers/aUtHoRiZaTiOn")) throw new Error("Authorization source path");
      const expected = row.entities.map(entity => ({...entity, start:7, end:Array.from(value).length}));
      equal(project(value, located.map(item => item.finding)), expected, "Authorization overlapping candidates");
      for (const entity of row.entities) {
        const result = api.scanAndTransformStructured(data, {transform:{default:{strategy:"redact"},entities:[entity.label]}});
        equal(result.data, {"👋/headers":{aUtHoRiZaTiOn:`Bearer [${entity.label}]`}}, "Authorization label selection");
      }
    }
    if (row.overlap && !findings.some(f => f.entityType === row.overlap)) throw new Error("missing generic overlap");
    equal(project(row.text, findings), row.entities, row.id);
    const data = {"a/b":row.text, array:[row.text], z:row.text};
    const located = api.scanStructured(data, row.config).findings;
    for (const path of ["/a~1b", "/array/0", "/z"]) {
      equal(project(row.text, located.filter(f => f.path === path).map(f => f.finding)), row.entities, row.id + path);
    }
    for (const test of row.transforms ?? []) {
      const result = api.scanAndTransform(row.text, {scan:row.config, transform:test.config});
      equal(result.text, test.text, row.id + " transform");
      equal(api.transform(row.text, findings, test.config), result, "explicit transform");
      for (const t of result.transformations) {
        equal(result.text.slice(t.outputUtf16Range.start,t.outputUtf16Range.end),t.replacement,"output UTF-16");
        equal(Array.from(result.text).slice(t.outputCodepointRange.start,t.outputCodepointRange.end).join(""),t.replacement,"output code points");
        equal(new TextDecoder().decode(new TextEncoder().encode(result.text).slice(t.outputByteRange.start,t.outputByteRange.end)),t.replacement,"output bytes");
        const source = row.text.slice(t.sourceUtf16Range.start,t.sourceUtf16Range.end);
        if (!findings.some(f => f.entityType === t.entityType && f.matchedText === source)) throw new Error("source range");
        if ("matchedText" in t || "finding" in t) throw new Error("record includes original PII");
      }
      const structured = api.scanAndTransformStructured(data, {scan:row.config, transform:test.config});
      equal(structured.data, {"a/b":test.text,array:[test.text],z:test.text}, "structured transform");
      equal(api.transformStructured(data, located, test.config), structured, "explicit structured transform");
    }
  }
}
