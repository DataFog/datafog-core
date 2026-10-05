// Shared by the installed Node and browser WASM package tests.
export function verifyEmailContext(api, records) {
  function equal(actual, expected, label) {
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      throw new Error(`Email context ${label}: ${JSON.stringify(actual)} != ${JSON.stringify(expected)}`);
    }
  }
  for (const row of records) {
    const findings = api.scan(row.text, row.config);
    const emails = findings.filter(finding => finding.entityType === "EMAIL");
    equal(emails.map(finding => ({
      text: finding.matchedText,
      start: finding.codepointRange.start,
      end: finding.codepointRange.end,
    })), row.matches, row.id);
    for (const finding of emails) {
      equal(row.text.slice(finding.utf16Range.start, finding.utf16Range.end), finding.matchedText, row.id);
      equal(new TextDecoder().decode(new TextEncoder().encode(row.text).slice(finding.byteRange.start, finding.byteRange.end)), finding.matchedText, row.id);
      equal(Array.from(row.text).slice(finding.codepointRange.start, finding.codepointRange.end).join(""), finding.matchedText, row.id);
    }
    const data = {source: row.text};
    equal(api.scanStructured(data, row.config).findings.map(located => located.finding), findings, row.id);
    for (const strategy of ["redact", "mask", "remove"]) {
      const policy = {default: {strategy}, entities: ["EMAIL"]};
      const envelope = {scan: row.config, transform: policy};
      const result = api.scanAndTransform(row.text, envelope);
      equal(result.text, row.outputs[strategy], `${row.id} ${strategy}`);
      equal(api.transform(row.text, findings, policy), result, row.id);
      result.transformations.forEach((record, index) => {
        equal(record.sourceByteRange, emails[index].byteRange, row.id);
        equal(record.sourceCodepointRange, emails[index].codepointRange, row.id);
        equal(row.text.slice(record.sourceUtf16Range.start, record.sourceUtf16Range.end), emails[index].matchedText, row.id);
        equal(result.text.slice(record.outputUtf16Range.start, record.outputUtf16Range.end), record.replacement, row.id);
      });
      equal(api.scanAndTransformStructured(data, envelope).data, {source: result.text}, row.id);
    }
    if (row.config.format === "text") equal(api.scan(row.text), findings, row.id);
  }
  for (const invalid of [null, 1, true, {}, "yaml", "ENV"]) {
    for (const call of [
      () => api.scan("", {format: invalid}),
      () => api.scanStructured({source: ""}, {format: invalid}),
    ]) {
      let rejected = false;
      try { call(); }
      catch (error) { rejected = error.code === "invalid_configuration" && error.path === "/format"; }
      if (!rejected) throw new Error("Invalid source format accepted");
    }
  }
}
