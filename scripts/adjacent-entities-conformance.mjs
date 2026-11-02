/** Shared installed Node/WASM checks for Core #218. Synthetic values only. */
export function verifyAdjacentEntities(api, fixture) {
  const check = (condition, message) => { if (!condition) throw Error(message); };
  const encoder = new TextEncoder(), decoder = new TextDecoder();
  for (const pair of fixture.pairs) for (const separator of pair.separators) {
    const text = `👋 é ${pair.left}${separator}${pair.right} 中`, findings = api.scan(text);
    for (const finding of findings) {
      check(decoder.decode(encoder.encode(text).slice(finding.byteRange.start, finding.byteRange.end)) === finding.matchedText, 'adjacent byte offsets');
      check([...text].slice(finding.codepointRange.start, finding.codepointRange.end).join('') === finding.matchedText, 'adjacent codepoint offsets');
      check(text.slice(finding.utf16Range.start, finding.utf16Range.end) === finding.matchedText, 'adjacent UTF-16 offsets');
    }
    for (const [entityType, value] of [[pair.leftType, pair.left], [pair.rightType, pair.right]]) {
      check(findings.some(f => f.entityType === entityType && f.matchedText === value), `missing exact adjacent ${entityType}`);
      check(api.transform(text, findings, {default: {strategy: 'redact'}, entities: [entityType]}).text ===
        text.replace(value, `[${entityType}]`), 'selected transformation swallowed neighbor');
    }
    check(api.transform(text, findings, {default: {strategy: 'redact'}}).text ===
      `👋 é [${pair.leftType}]${separator}[${pair.rightType}] 中`, 'adjacent transform failed or changed separator');
    const workspace = api.Workspace.create(new Uint8Array(16).fill(1), new Uint8Array(16).fill(2));
    try {
      const plan = workspace.prepareReview([{text, scannerFindings: findings.map(f => ({entityType: f.entityType,
        matchedText: f.matchedText, range: {unit: 'utf8_bytes', ...f.byteRange}})), userSelections: []}], new Uint8Array(16).fill(3));
      const preview = plan.preview().text, heads = plan.heads();
      workspace.activateSaved(plan, heads.expectedHead, heads.candidateHead);
      check(workspace.restore(preview) === text, 'adjacent workspace review or restoration failed');
    } finally { workspace.close(); }
  }
  for (const intact of fixture.intact) {
    const findings = api.scan(intact.text, {entities: [intact.type]});
    check(findings.length === 1 && findings[0].matchedText === intact.text, 'split a valid isolated value');
  }
}
