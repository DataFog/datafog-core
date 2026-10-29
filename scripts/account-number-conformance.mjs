// Shared synthetic rule contract, run against installed Node and browser WASM.
export function verifyAccountNumber(api, records) {
  const equal = (actual, expected, name) => {
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      throw new Error(`${name}: ${JSON.stringify(actual)} != ${JSON.stringify(expected)}`);
    }
  };
  function project(text, findings) {
    return findings.filter(f => f.entityType === "ACCOUNT_NUMBER").map(f => {
      equal(new TextDecoder().decode(new TextEncoder().encode(text).slice(f.byteRange.start, f.byteRange.end)), f.matchedText, "account bytes");
      equal(Array.from(text).slice(f.codepointRange.start, f.codepointRange.end).join(""), f.matchedText, "account codepoints");
      equal(text.slice(f.utf16Range.start, f.utf16Range.end), f.matchedText, "account UTF-16");
      equal(f.detectorName, "datafog-core/account-number", "account provenance");
      equal(f.confidence, undefined, "rule confidence");
      return {label:f.entityType, text:f.matchedText, start:f.codepointRange.start, end:f.codepointRange.end};
    });
  }
  for (const row of records) {
    equal(project(row.text, api.scan(row.text)), row.entities, row.id);
    equal(project(row.text, api.scan(row.text, {entities:["ACCOUNT_NUMBER"]})), row.entities, row.id + " selected");
    const data = {note:row.text};
    equal(project(row.text, api.scanStructured(data).findings.map(f => f.finding)), row.entities, row.id + " structured");
    const cases = row.transforms ?? [{config:{default:{strategy:"redact"},entities:["ACCOUNT_NUMBER"]},text:row.text}];
    for (const test of cases) {
      equal(api.scanAndTransform(row.text, {transform:test.config}).text, test.text, row.id + " transform");
      equal(api.scanAndTransformStructured(data, {transform:test.config}).data, {note:test.text}, row.id + " structured transform");
    }
  }
  equal(api.scanStructured({"account number":"12345678"}).findings.filter(f => f.finding.entityType === "ACCOUNT_NUMBER"), [], "field keys supply no text context");
}
