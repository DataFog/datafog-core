/** Shared installed-package workspace checks; all values are synthetic. */
export async function verifyWorkspace(api, counters) {
  const check = (condition, label) => { if (!condition) throw new Error(label); };
  const throws = (callback, code) => {
    try { callback(); } catch (error) { check(error.code === code, `workspace error category: ${code}`); check(error.message === code, "content-free workspace error"); return; }
    throw new Error(`missing workspace rejection: ${code}`);
  };
  const equal = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
  const id = value => new Uint8Array(16).fill(value);
  await api.init();
  const w = api.Workspace.create(id(1), id(2));
  const finding = { entityType: "Client_Name", matchedText: "Jane Smith", range: { unit: "utf8_bytes", start: 13, end: 23 } };
  const source = "2026-01-06 | Jane Smith | -40.00 USD";
  const before = w.exportBundle();
  let change = w.prepare(source, [finding], id(3));
  const preview = change.preview();
  check(equal(w.exportBundle(), before), "preparation preserves committed state");
  check(equal(change.exportCandidateBundle(), change.exportCandidateBundle()), "pending candidate exports are immutable");
  const marker = preview.text.match(/\[\[DFP1:.*?\]\]/)?.[0];
  check(marker?.includes(":Client_Name:1]]"), "caller-defined exact type code");
  check(preview.text === source.replace(finding.matchedText, marker), "non-selected text preservation");
  throws(() => w.restore(marker), "invalid_marker");
  const saved = change.exportCandidateBundle();
  const heads = change.heads();
  w.activateSaved(change, heads.expectedHead, heads.candidateHead);
  throws(() => change.exportCandidateBundle(), "inactive_plan");
  throws(() => w.activateSaved(change, heads.expectedHead, heads.candidateHead), "inactive_plan");
  check(w.restore(marker) === finding.matchedText, "exact local restoration");
  check(w.restore("Client_Name_1") === "Client_Name_1", "short labels are never restoration keys");
  for (const bad of [marker.replace("DFP1", "DFP2"), marker.replace(":Client_Name:", ":PERSON:"), "[[DFP1:damaged"]) {
    throws(() => w.restore(`${marker} ${bad}`), "invalid_marker");
  }
  throws(() => w.prepare(marker, [], id(4)), "invalid_marker");
  throws(() => w.prepare(source, [finding], id(3)), "invalid_operation_id");
  change = w.prepare(source, [finding], id(4));
  check(change.heads().expectedHead.revision === change.heads().candidateHead.revision, "no-change head");
  w.activateSaved(change, change.heads().expectedHead, change.heads().candidateHead);
  throws(() => change.preview(), "inactive_plan");
  const reopened = api.Workspace.openBundle(saved);
  change = reopened.prepare(source, [finding], id(5));
  check(change.preview().text === preview.text, "reopen preserves exact full marker");
  throws(() => w.activateSaved(change, change.heads().expectedHead, change.heads().candidateHead), "workspace_conflict");
  change.discard(); throws(() => change.preview(), "inactive_plan");
  const selection = api.workspaceByteRangeFromUtf16("👋 é Jane", 6, 10);
  check(selection.start === 9 && selection.end === 13 && selection.unit === "utf8_bytes", "Core UTF-16 conversion");
  throws(() => api.workspaceByteRangeFromUtf16("👋 Jane", 1, 7), "invalid_finding");
  throws(() => w.prepare("\uD800", [], id(8)), "invalid_finding");
  for (const entityType of ["", "bad:type", "_PERSON", " PERSON", "x".repeat(65)]) throws(() => w.prepare(source, [{...finding, entityType}], id(6)), "invalid_finding");
  throws(() => w.prepare(source, [{...finding, matchedText: "wrong"}], id(6)), "invalid_finding");
  throws(() => w.prepare(source, [{...finding, range: {...finding.range, unit: "utf16"}}], id(6)), "invalid_finding");
  throws(() => w.prepare(source, [{...finding, confidence: -1}], id(6)), "invalid_finding");
  throws(() => w.prepare("x".repeat(1024 * 1024 + 1), [], id(6)), "resource_limit");
  throws(() => w.prepare("", Array(10001).fill(finding), id(6)), "resource_limit");
  const repeated = { ...finding, matchedText: "x".repeat(64 * 1024), range: { unit: "utf8_bytes", start: 0, end: 64 * 1024 } };
  throws(() => w.prepare("x".repeat(64 * 1024), Array(1024).fill(repeated), id(6)), "resource_limit");
  throws(() => w.prepare(source, [{ ...finding, confidence: { nested: Array(10000).fill(1) } }], id(6)), "invalid_finding");
  check(equal(w.exportBundle(), saved), "invalid operations preserve committed bundle");
  for (const malformed of [saved.slice(0, -1), new Uint8Array([...saved, 0]), new Uint8Array([0xbf, ...saved.slice(1)]), new Uint8Array([0xa7, 0x18, 0, ...saved.slice(2)])]) {
    throws(() => api.Workspace.openBundle(malformed), "invalid_snapshot");
  }
  let duplicate = saved.slice(); duplicate[3] = 0; throws(() => api.Workspace.openBundle(duplicate), "invalid_snapshot");
  for (const [name, fixture] of Object.entries(counters)) {
    const bytes = Uint8Array.from(fixture.bundleHex.match(/../g), pair => Number.parseInt(pair, 16));
    const high = api.Workspace.openBundle(bytes);
    check(high.head().revision === fixture.revision, "lossless exposed uint64 revision");
    check(equal(bytes, high.exportBundle()) && high.restore(fixture.marker) === "Jane", "CBOR uint64 fixture round trip");
    const fresh = { entityType: "PERSON", matchedText: "John", range: { unit: "utf8_bytes", start: 0, end: 4 } };
    if (name === "maximum") throws(() => high.prepare("John", [fresh], id(6)), "resource_limit");
    else {
      const p = high.prepare("John", [fresh], id(6));
      check(p.heads().candidateHead.revision === String(BigInt(fixture.revision) + 1n), "lossless uint64 increment");
      check(p.preview().text.endsWith(`:PERSON:${BigInt(fixture.counter) + 1n}]]`), "lossless uint64 counter");
      p.discard();
    }
    high.close();
  }
  change = w.prepare("", [], id(7)); w.close();
  throws(() => change.preview(), "closed_workspace");
  reopened.close();
  return true;
}
