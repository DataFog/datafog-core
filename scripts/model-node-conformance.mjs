// Explicit installed-package test; missing config, assets or package fails.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

const config = JSON.parse(readFileSync(process.argv[2], "utf8"));
const api = await import(pathToFileURL(process.argv[3]).href);
const { ModelSession, transform, transformStructured } = api;
const texts = [
  "John Smith lives at 123 Main Street, Boston, MA 02110. Email john@example.com.",
  "The weather is clear.",
  "👋 Contact May Smith at 123 Main Street, Boston, MA 02110.",
  "e\u0301 👋 John Smith lives at 123 Main Street. John Smith lives at 123 Main Street.",
  "Ordinary words without private details. ".repeat(160) + "John Smith lives at 123 Main Street.",
];
const executable = config.bundle + (process.platform === "win32" ? "/datafog-pii.exe" : "/datafog-pii");
const reference = spawnSync(executable, [config.bundle], {
  input: texts.map(text => JSON.stringify({text}) + "\n").join(""),
  encoding: "utf8", timeout: config.limits.timeout_ms,
});
assert.equal(reference.status, 0);
const native = reference.stdout.trim().split("\n").map(line => JSON.parse(line).findings);
const session = await ModelSession.open(config);
try {
  const identity = session.inspect().configuration_id;
  await session.warmUp();
  for (let i = 0; i < texts.length; i++) {
    const text = texts[i];
    const result = await session.scan(text, "model_only");
    assert.equal(result.configurationId, identity);
    assert.equal(result.findings.length, native[i].length);
    for (let j = 0; j < result.findings.length; j++) {
      const finding = result.findings[j], expected = native[i][j];
      assert.equal(finding.byteRange.start, expected.start);
      assert.equal(finding.byteRange.end, expected.end);
      assert.equal(Buffer.from(text).subarray(finding.byteRange.start, finding.byteRange.end).toString(), finding.matchedText);
      assert.equal(text.slice(finding.utf16Range.start, finding.utf16Range.end), finding.matchedText);
      assert.equal([...text].slice(finding.codepointRange.start, finding.codepointRange.end).join(""), finding.matchedText);
      assert.equal(result.evidence[j].sources[0].native_label, expected.label);
      assert.ok(Math.abs(result.evidence[j].sources[0].raw_score.value - expected.raw_score) <= Number.EPSILON);
      assert.ok(Math.abs(finding.confidence - expected.confidence) <= 2 ** -24);
    }
  }
  const result = await session.scan(texts[0], "combined", {diagnostics: true});
  assert.ok(result.diagnostics);
  assert.ok(result.evidence.some(item => item.sources.length > 1));
  const protectedText = transform(texts[0], result.findings, {default: {strategy: "redact"}}).text;
  assert.ok(protectedText.includes("[STREET_ADDRESS]"));
  assert.ok(!protectedText.includes("john@example.com"));
  const data = {first_name: "John", note: texts[2], keep: 12, array: [texts[1]]};
  const structured = await session.scanStructured(data, "combined");
  const protectedData = transformStructured(data, structured.findings, {default: {strategy: "redact"}}).data;
  assert.equal(protectedData.keep, 12);
  assert.ok(protectedData.note.includes("[STREET_ADDRESS]"));
  assert.deepEqual(structured.evidence.map(item => item.finding_index), structured.findings.map((_,i) => i));
  await session.health();
} finally {
  await session.close();
}
await session.close();
await assert.rejects(session.scan("", "combined"), error => error.code === "model_session_closed");
await assert.rejects(ModelSession.open({...config, manifest_sha256: "0".repeat(64)}), error => error.reason === "integrity_mismatch");
console.log("Installed Node pinned-model parity, negatives, Unicode, long input, reuse, structured composition and cleanup passed.");

const {ModelCancellation} = api;
const lifecycle = await api.ModelSession.open(config);
try {
  const cancellation = new ModelCancellation();
  cancellation.cancel();
  await assert.rejects(lifecycle.scan("private@example.test", "combined", {cancellation}),
    error => error.code === "model_cancelled");
  assert.equal(lifecycle.inspect().state, "ready");
  const first = lifecycle.scan("John Smith lives at 123 Main Street.", "model_only");
  await assert.rejects(lifecycle.scan("second", "model_only"), error => error.code === "model_busy");
  await first;
} finally { await lifecycle.close(); }
