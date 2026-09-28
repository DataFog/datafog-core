// Shared installed-package checks, executed in Node and in a real browser.
export function verifyGerman(api, records) {
  const equal = (actual, expected, message) => {
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      throw new Error(`${message}: ${JSON.stringify(actual)} != ${JSON.stringify(expected)}`);
    }
  };
  function project(text, findings) {
    return findings.filter(f => f.entityType.startsWith("DE_")).map(f => {
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
  const split = {"Steuer-ID":"12345678901", a:"Steuer-ID", b:"12345678901", c:["Passport","C12345678"], d:"DE4450010517",e:"5407324931"};
  equal(api.scanStructured(split,{locale:"de"}).findings.filter(f => f.finding.entityType.startsWith("DE_")),[],"cross-field context");
  for (const config of [{locale:""},{locale:" \t"},{locale:null},{locale:1},{locales:["de"]}]) {
    for (const call of [() => api.scan("",config),() => api.scanStructured({},config)]) {
      let rejected = false;
      try { call(); } catch (error) { rejected = error.code === "invalid_configuration"; }
      if (!rejected) throw new Error("malformed scan config accepted");
    }
  }
  equal(api.scanAndTransform("DE44500105175407324931", {transform:{default:{strategy:"redact"},entities:["DE_IBAN"]}}).text,"DE44500105175407324931","selection activated detector");
  for (const [text,label,generic,expected] of [
    ["DE44\t4111111111111111\t11","DE_IBAN","CREDIT_CARD","[DE_IBAN]"],
    ["DE 123456789","DE_VAT_ID","SSN","[DE_VAT_ID]"],
    ["Steuer-ID 12345678901","DE_TAX_ID","PHONE","Steuer-ID [DE_TAX_ID]"],
    ["DE-10115","DE_POSTAL_CODE","ZIP_CODE","[DE_POSTAL_CODE]"],
  ]) {
    const findings = api.scan(text,{locale:"de"});
    if (![label,generic].every(label => findings.some(f => f.entityType === label))) throw new Error("missing overlap");
    equal(api.scanAndTransform(text,{scan:{locale:"de"},transform:{default:{strategy:"redact"}}}).text,expected,"overlap selection");
  }
}

export async function verifyGermanProviders(records, manager, tokenManager, context) {
  for (const row of records.filter(r => r.sample)) {
    const entities = [...new Set(row.entities.map(e => e.label))];
    const pseudonyms = await manager.scanAndTransform(row.text,{scan:row.config,transform:{default:{strategy:"pseudonymize",key_ref:"german"},entities}});
    if (pseudonyms.transformations.length !== row.entities.length || pseudonyms.text === row.text) throw new Error("German pseudonymization");
    const tokens = await tokenManager.scanAndTransform(row.text,{scan:row.config,transform:{default:{strategy:"tokenize",token_ref:"german"},entities}},context);
    if (tokens.transformations.length !== row.entities.length) throw new Error("German tokenization");
    const restored = await tokenManager.restore(tokens.text,context);
    if (restored.text !== row.text) throw new Error("German restore");
  }
}
