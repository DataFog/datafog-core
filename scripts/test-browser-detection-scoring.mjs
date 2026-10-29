import {test} from "node:test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {createHash} from "node:crypto";
import {matchSpans, validateSpan, scoreCorpus} from "./browser-detection-scoring.mjs";
const span = (start, end, entityType = "PERSON") => ({entityType, byteRange: {start, end}});
test("overlap is maximum one-to-one, including an augmenting path", () => {
  assert.equal(matchSpans([span(0, 4), span(6, 10)], [span(0, 10), span(0, 4)], "overlap").tp, 2);
  const result = matchSpans([span(0, 4), span(6, 10)], [span(0, 10)], "overlap");
  assert.equal(result.tp, 1); assert.equal(result.fn, 1);
});
test("duplicate predictions and wrong labels do not inflate recall", () => {
  const result = matchSpans([span(0, 4)], [span(0, 4), span(0, 4), span(0, 4, "ADDRESS")], "exact");
  assert.equal(result.tp, 1); assert.equal(result.fp, 2);
});
test("full addresses remain exact misses against street lines", () => {
  assert.equal(matchSpans([span(0, 10, "ADDRESS")], [span(0, 25, "ADDRESS")], "exact").tp, 0);
  assert.equal(matchSpans([span(0, 10, "ADDRESS")], [span(0, 25, "ADDRESS")], "overlap").tp, 1);
});
test("invalid byte/codepoint/UTF16 spans fail instead of being scored", () => {
  const value = {...span(5, 9), matchedText: "Jane", utf16Range: {start: 3, end: 7}, codepointRange: {start: 2, end: 6}};
  validateSpan("👋 Jane", value);
  assert.throws(() => validateSpan("👋 Jane", {...value, byteRange: {start: 1, end: 9}}));
  assert.throws(() => validateSpan("👋 Jane", {...value, utf16Range: {start: 2, end: 6}}));
});
test("the pinned synthetic corpus has valid unique records and explicit capability gaps", async () => {
  const corpus = JSON.parse(await readFile(new URL("../fixtures/browser-detection/qualification-v1.json", import.meta.url)));
  assert.equal(new Set(corpus.records.map(record => record.id)).size, corpus.records.length);
  const empty = corpus.records.map(record => ({id: record.id, findings: []}));
  const scored = scoreCorpus(corpus, empty);
  assert.equal(scored.shipping.exact.totals.PERSON.tp, 0);
  assert(scored.shipping.exact.totals.PERSON.fn >= 20);
  assert.equal(scored.shipping.exact.totals.ACCOUNT_NUMBER.fn, 1);
  assert.equal(scored.shipping.exact.totals.PERSON.precision, null);
});

test("the committed receipt reproduces scores and binds the scorer and corpus", async () => {
  const root = new URL("../", import.meta.url);
  const summary = JSON.parse(await readFile(new URL("baselines/browser-detection-small-preparation.json", root)));
  const raw = await readFile(new URL("baselines/browser-detection-small-preparation.jsonl", root));
  const digest = value => createHash("sha256").update(value).digest("hex");
  assert.equal(digest(raw), summary.observationsSha256);
  for (const file of ["scripts/browser-detection-scoring.mjs", "fixtures/browser-detection/qualification-v1.json"]) assert.equal(digest(await readFile(new URL(file, root))), summary.inputs[file], `receipt scoring input changed: ${file}`);
  const corpus = JSON.parse(await readFile(new URL("fixtures/browser-detection/qualification-v1.json", root)));
  const observations = raw.toString().trim().split("\n").map(line => JSON.parse(line));
  const scores = scoreCorpus(corpus, observations);
  for (const [view, modes] of Object.entries(scores)) for (const [mode, value] of Object.entries(modes)) assert.deepEqual(value.totals, summary.scores[view][mode]);
  assert.equal(summary.qualificationComplete, false);
  assert.equal(summary.sourceDirty, false);
  assert.equal(summary.passedContracts, true);
});

test("the current merged-feature receipt preserves raw scoring and separate approved-value recall", async () => {
  const root=new URL("../",import.meta.url);
  const summary=JSON.parse(await readFile(new URL("baselines/browser-detection-small-current.json",root)));
  const raw=await readFile(new URL("baselines/browser-detection-small-current.jsonl",root));
  const digest=value=>createHash("sha256").update(value).digest("hex");
  assert.equal(digest(raw),summary.observationsSha256);
  for(const file of ["scripts/browser-detection-scoring.mjs","fixtures/browser-detection/qualification-v1.json"])assert.equal(digest(await readFile(new URL(file,root))),summary.inputs[file]);
  const corpus=JSON.parse(await readFile(new URL("fixtures/browser-detection/qualification-v1.json",root)));
  const observations=raw.toString().trim().split("\n").map(line=>JSON.parse(line));
  const scores=scoreCorpus(corpus,observations);
  for(const [view,modes] of Object.entries(scores))for(const [mode,value] of Object.entries(modes))assert.deepEqual(value.totals,summary.scores[view][mode]);
  assert.equal(summary.sourceDirty,false);assert.equal(summary.passedContracts,true);assert.equal(summary.qualificationComplete,false);
  assert.equal(summary.workspaceWorkflow.passed,true);
  assert.equal(summary.workspaceWorkflow.recall.length,6);
  const expected=corpus.records.reduce((sum,record)=>sum+record.gold.length,0);
  const positive=corpus.records.filter(record=>record.gold.length).length;
  for(const phase of summary.workspaceWorkflow.recall) {
    assert.equal(phase.checked,expected);assert.equal(phase.restoredRecords,positive);
    assert.deepEqual(phase.misses,[]);assert.equal(phase.readOnly,true);
  }
  const account=observations.find(record=>record.id==="account-number-gap");
  assert(account.findings.some(f=>f.entityType==="ACCOUNT_NUMBER"&&f.detectorName==="datafog-core/account-number"));
});
