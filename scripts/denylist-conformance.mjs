/** Installed Node and browser WASM exercise the same public literal-deny contract. */
export function verifyDenylist(api) {
  const check = (condition, message) => { if (!condition) throw Error(message); };
  const denied = (text, deny) => api.scan(text, {entities: ['EMAIL'], deny});
  const text = '👋 é Key ſecret Project Atlas';
  const findings = denied(text, [
    {phrase: 'key', case_sensitive: false},
    {phrase: 'secret', case_sensitive: false},
    {phrase: 'Project Atlas'},
  ]);
  check(findings.length === 3, 'deny rules must run independently of entity selection');
  const encoder = new TextEncoder(), decoder = new TextDecoder();
  for (const finding of findings) {
    check(finding.entityType === 'DENYLIST' && finding.detectorName === 'datafog-core/denylist', 'deny provenance');
    check(decoder.decode(encoder.encode(text).slice(finding.byteRange.start, finding.byteRange.end)) === finding.matchedText, 'deny byte offsets');
    check([...text].slice(finding.codepointRange.start, finding.codepointRange.end).join('') === finding.matchedText, 'deny codepoint offsets');
    check(text.slice(finding.utf16Range.start, finding.utf16Range.end) === finding.matchedText, 'deny UTF-16 offsets');
  }
  check(denied('Atlas atlas _Atlas Atlas2 中Atlas', [{phrase: 'Atlas'}]).length === 1, 'default exact case and Unicode word boundaries');
  check(denied('xA+B.y', [{phrase: 'A+B.', boundary: 'substring'}])[0]?.matchedText === 'A+B.', 'literal punctuation and substring matching');
  check(denied('x(A) (A)y (A)', [{phrase: '(A)'}]).length === 1, 'boundaries apply outside punctuation');
  check(denied('STRASSE', [{phrase: 'straße', case_sensitive: false}]).length === 0, 'simple folding must not expand sharp s');
  check(denied('é', [{phrase: 'é'}]).length === 0, 'literal matching must not normalize');
  const transform = {default: {strategy: 'redact'}, entities: ['EMAIL'], allow: {
    exact: {DENYLIST: ['Project Atlas'], EMAIL: ['jane@example.com']},
    regex: {DENYLIST: [{pattern: '.*'}]},
  }};
  const original = 'Project Atlas jane@example.com';
  const config = {scan: {entities: ['EMAIL'], deny: [{phrase: 'Project Atlas'}]}, transform};
  check(api.scanAndTransform(original, config).text === '[DENYLIST] jane@example.com', 'deny must override filters and allowlists while preserving ordinary allowlisting');
  check(api.transform(original, api.scan(original, config.scan), transform).text === '[DENYLIST] jane@example.com', 'explicit transform deny precedence');
  const legacy = {...api.scan('Project Atlas', {deny: [{phrase: 'Project Atlas'}]})[0], detectorName: 'custom/legacy'};
  check(api.transform('Project Atlas', [legacy], transform).text === 'Project Atlas', 'custom DENYLIST entity retains legacy filters');
  const document = {note: text};
  const structured = api.scanStructured(document, {deny: [{phrase: 'Project Atlas'}]});
  check(structured.findings.length === 1 && structured.findings[0].path === '/note' &&
    structured.findings[0].finding.matchedText === 'Project Atlas', 'structured literal deny');
  const structuredTransform = api.scanAndTransformStructured({note: original}, {
    scan: {deny: [{phrase: 'Project Atlas'}]}, transform,
  });
  check(structuredTransform.data.note === '[DENYLIST] jane@example.com', 'structured deny precedence');
  for (const deny of [null, 'Atlas', [{}], [{phrase: ''}], [{phrase: '  '}], [{phrase: 1}],
    [{phrase: 'Atlas', case_sensitive: 'false'}], [{phrase: 'Atlas', boundary: 'regex'}]]) {
    let rejected = false;
    try { denied('Atlas', deny); } catch { rejected = true; }
    check(rejected, 'invalid deny configuration must fail');
  }
}
