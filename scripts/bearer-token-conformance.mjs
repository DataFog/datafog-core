// Shared installed-package checks, executed in Node and in a real browser.
export function verifyBearerToken(api, records) {
  const equal = (actual, expected, message) => {
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      throw new Error(`${message}: ${JSON.stringify(actual)} != ${JSON.stringify(expected)}`);
    }
  };
  function project(text, findings) {
    return findings.filter(f => f.entityType === "BEARER_TOKEN").map(f => {
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
  for (const row of records) {
    const findings = api.scan(row.text, row.config);
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
  const data = {"👋/~":{"aUtHoRiZaTiOn":"  bEaReR abc._~+/==\t"}, Authorization:["Bearer hidden"], "X-Authorization":"Bearer hidden", a:"Authorization:", b:"Bearer hidden", invalid:{Authorization:"Bearer bad!value"}};
  const found = api.scanStructured(data).findings.filter(f => f.finding.entityType === "BEARER_TOKEN");
  equal(found.length,1,"structured header count");
  equal(found[0].path,"/👋~1~0/aUtHoRiZaTiOn","structured header path");
  equal(project(data["👋/~"].aUtHoRiZaTiOn,[found[0].finding]),[{label:"BEARER_TOKEN",text:"abc._~+/==",start:9,end:19}],"structured header offsets");
  const expected = {...data,"👋/~":{aUtHoRiZaTiOn:"  bEaReR [BEARER_TOKEN]\t"}};
  const transformed = api.scanAndTransformStructured(data,{transform:{default:{strategy:"redact"},entities:["BEARER_TOKEN"]}}).data;
  equal(Object.keys(transformed).sort(),Object.keys(expected).sort(),"structured header keys");
  for (const key of Object.keys(expected)) equal(transformed[key],expected[key],"structured header redaction " + key);
  equal(api.scan("Bearer abc123").filter(f=>f.entityType === "BEARER_TOKEN"),[],"bare header value is not text context");

}

export async function verifyBearerTokenProviders(records, manager, tokenManager, context) {
  for (const row of records.filter(r => r.sample)) {
    const entities = [...new Set(row.entities.map(e => e.label))];
    const pseudonyms = await manager.scanAndTransform(row.text,{scan:row.config,transform:{default:{strategy:"pseudonymize",key_ref:"jwt"},entities}});
    if (pseudonyms.transformations.length !== row.entities.length || pseudonyms.text === row.text) throw new Error("BearerToken pseudonymization");
    const tokens = await tokenManager.scanAndTransform(row.text,{scan:row.config,transform:{default:{strategy:"tokenize",token_ref:"jwt"},entities}},context);
    if (tokens.transformations.length !== row.entities.length) throw new Error("BearerToken tokenization");
    const restored = await tokenManager.restore(tokens.text,context);
    if (restored.text !== row.text) throw new Error("BearerToken restore");
  }
}
