import assert from "node:assert/strict";
import {scoreCorpus} from "./browser-detection-scoring.mjs";
import {validateSdkSpan} from "./browser-detection-contract.mjs";

// Accuracy remains measured evidence. This gate checks complete execution and
// SDK contracts, not a newly invented release accuracy target.
export function validateCoreSdkQualification(receipt, corpus, model) {
  assert.equal(receipt.kind, "core-sdk-browser-current-measurement");
  assert.equal(receipt.extensionAcceptance, false);
  assert.equal(receipt.qualificationComplete, false);
  assert.equal(receipt.passedContracts, true);
  assert.equal(receipt.failure, undefined);
  assert.deepEqual(receipt.externalRequests, []);
  assert.deepEqual(receipt.pageErrors, []);
  assert.equal(receipt.browser?.extension, true);
  assert.equal(receipt.browser?.manifest?.manifest_version, 3);
  assert.deepEqual(receipt.model, model);
  assert.match(receipt.package?.sha256 || "", /^[a-f0-9]{64}$/);
  assert.match(receipt.consumerLockSha256 || "", /^[a-f0-9]{64}$/);
  for (const [name, expected] of Object.entries(model.files)) {
    assert.deepEqual(receipt.assets?.[`models/${name}`], expected);
  }
  assert.deepEqual(receipt.options, corpus.model);
  const scores = scoreCorpus(corpus, receipt.observations);
  assert.deepEqual(receipt.scores, scores);
  const records = new Map(corpus.records.map(record => [record.id, record]));
  for (const observation of receipt.observations) {
    for (const finding of observation.findings) validateSdkSpan(records.get(observation.id).text, finding);
  }
  assert.equal(receipt.workspaceWorkflow?.passed, true);
  const names = ["after explicit saved approvals", "browser process restart without model",
    "password backup import, fresh encryption and head", "password recovered browser restart",
    "recovery backup import, fresh encryption and head", "recovery recovered browser restart"];
  assert.deepEqual(receipt.workspaceWorkflow.recall.map(phase => phase.name), names);
  const checked = corpus.records.reduce((sum, record) => sum + record.gold.length, 0);
  const restored = corpus.records.filter(record => record.gold.length).length;
  for (const phase of receipt.workspaceWorkflow.recall) {
    assert.equal(phase.checked, checked);
    assert.equal(phase.restoredRecords, restored);
    assert.deepEqual(phase.misses, []);
    assert.equal(phase.readOnly, true);
  }
  return {passed: true, scope: "Core SDK contracts and explicit-approved-value recall",
    extensionAcceptance: false, accuracyThresholdApplied: false,
    recordsMeasured: corpus.records.length,
    exactAccuracy: scores.shipping.exact.totals,
    recallPhases: receipt.workspaceWorkflow.recall};
}
