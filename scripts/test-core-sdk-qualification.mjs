import assert from "node:assert/strict";
import {test} from "node:test";
import {readFile} from "node:fs/promises";
import {MODEL} from "../bindings/wasm/models/model.mjs";
import {scoreCorpus} from "./browser-detection-scoring.mjs";
import {validateCoreSdkQualification} from "./core-sdk-qualification.mjs";

const root = new URL("../", import.meta.url);
const corpus = JSON.parse(await readFile(new URL("fixtures/browser-detection/qualification-v1.json", root)));
const historic = JSON.parse(await readFile(new URL("baselines/browser-detection-small-current.json", root)));
historic.observations = (await readFile(new URL("baselines/browser-detection-small-current.jsonl", root), "utf8")).trim().split("\n").map(JSON.parse);
historic.scores = scoreCorpus(corpus, historic.observations);
// A copy exercises gate semantics; it is not a fresh qualification receipt.
const candidate = () => ({...structuredClone(historic), kind: "core-sdk-browser-current-measurement", extensionAcceptance: false});
test("SDK gate accepts complete contracts while retaining actual imperfect accuracy", () => {
  const result = validateCoreSdkQualification(candidate(), corpus, MODEL);
  assert.equal(result.passed, true);
  assert.equal(result.accuracyThresholdApplied, false);
  assert(result.exactAccuracy.ADDRESS.fn > 0);
  assert.equal(result.recordsMeasured, 64);
  assert.equal(result.extensionAcceptance, false);
});
test("SDK gate rejects incomplete scans, skipped recall, invalid ranges, wrong assets and inflated scores", () => {
  for (const mutate of [
    value => value.observations.pop(),
    value => value.workspaceWorkflow.recall.pop(),
    value => value.workspaceWorkflow.recall[0].checked--,
    value => value.workspaceWorkflow.recall[0].misses.push({id: "missing"}),
    value => value.observations.find(row => row.findings.length).findings[0].utf16Range.start++,
    value => value.model.revision = "unpinned",
    value => value.assets["models/tokenizer.json"].sha256 = "0".repeat(64),
    value => value.scores.shipping.exact.totals.ADDRESS.fn = 0,
    value => value.passedContracts = false,
    value => value.externalRequests.push("https://example.test/unexpected"),
    value => value.extensionAcceptance = true,
  ]) {
    const value = candidate(); mutate(value);
    assert.throws(() => validateCoreSdkQualification(value, corpus, MODEL));
  }
});
