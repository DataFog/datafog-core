import test from 'node:test';
import assert from 'node:assert/strict';
import {BrowserWorkspace} from '../bindings/wasm/cnn/workspace.mjs';

function deferred() { let resolve; const promise = new Promise(done => {resolve = done;}); return {promise, resolve}; }
function fixture() {
  let discarded = 0, activated = 0, lastInput;
  const workspace = {
    closed: false,
    prepareReview(input) {
      lastInput = input;
      return {preview: () => ({text: '[PERSON_1]'}), previews: () => [], heads: () => ({}), discard() {discarded++;}};
    },
    async commit() { activated++; return {previews: [], head: {}, recordRevision: 1n}; },
    restore: () => 'Alice Smith', restoreReport: () => ({text: 'Alice Smith', issues: []}), head: () => ({}),
    close() {this.closed = true; return Promise.resolve();},
  };
  const detector = {
    enabled: true,
    async scan() { return {findings: [{entityType: 'PERSON', matchedText: 'Alice Smith', byteRange: {start: 0, end: 11}}]}; },
    async setEnabled(value) {this.enabled = value;}, async close() {},
  };
  return {workspace, detector, owner: BrowserWorkspace.adopt({workspace, detector}), counts: () => ({discarded, activated}), input: () => lastInput};
}

test('toggle revokes an approved review, retains restoration, and allows a fresh review', async () => {
  const f = fixture(), review = await f.owner.prepare('Alice Smith', new Uint8Array(16));
  assert.equal(f.input()[0].scannerFindings[0].range.unit, 'utf8_bytes');
  await f.owner.setCnnEnabled(false);
  assert.throws(() => review.preview(), {code: 'stale_review'});
  await assert.rejects(f.owner.commit(review, {approved: true}), {code: 'stale_review'});
  assert.equal(f.owner.restore('[PERSON_1]'), 'Alice Smith');
  const next = await f.owner.prepare('Alice Smith', new Uint8Array(16));
  await f.owner.commit(next, {approved: true});
  assert.deepEqual(f.counts(), {discarded: 1, activated: 1});
});

test('a late inference result cannot become a review after toggle', async () => {
  const f = fixture(), pending = deferred(); f.detector.scan = () => pending.promise;
  const scanning = f.owner.prepare('Alice Smith', new Uint8Array(16));
  await f.owner.setCnnEnabled(false);
  pending.resolve({findings: []});
  await assert.rejects(scanning, {code: 'stale_review'});
  assert.equal(f.input(), undefined);
});

test('toggle during submitted persistence closes before activation and releases no result', async () => {
  const f = fixture(), saved = deferred();
  f.workspace.commit = async () => { await saved.promise; if (f.workspace.closed) throw Object.assign(new Error(), {code: 'closed'}); return {}; };
  const review = await f.owner.prepare('Alice Smith', new Uint8Array(16));
  const saving = f.owner.commit(review, {approved: true});
  const toggling = f.owner.setCnnEnabled(false);
  assert.equal(f.workspace.closed, true);
  await assert.rejects(toggling, {code: 'reopen_required'});
  saved.resolve();
  await assert.rejects(saving, {code: 'closed'});
  assert.equal(f.counts().activated, 0);
});

test('approval must be literal true and request inputs are snapshotted before inference', async () => {
  const f = fixture(), pending = deferred(); f.detector.scan = () => pending.promise;
  const selections = [{entityType: 'PERSON', matchedText: 'Alice', range: {unit: 'utf8_bytes', start: 0, end: 5}}];
  const scanning = f.owner.prepare('Alice Smith', new Uint8Array(16), {userSelections: selections});
  selections[0].matchedText = 'changed'; pending.resolve({findings: []});
  const review = await scanning;
  assert.equal(f.input()[0].userSelections[0].matchedText, 'Alice');
  await assert.rejects(f.owner.commit(review, {approved: 'true'}), {code: 'approval_required'});
  assert.equal(f.counts().activated, 0);
});

test('lock revokes immediately even when cleanup remains pending', async () => {
  const f = fixture(), cleanup = deferred();
  f.detector.close = () => cleanup.promise;
  const review = await f.owner.prepare('Alice Smith', new Uint8Array(16));
  const closing = f.owner.lock();
  assert.equal(f.workspace.closed, true);
  assert.throws(() => review.preview(), {code: 'stale_review'});
  assert.throws(() => f.owner.restore('[PERSON_1]'), {code: 'workspace_closed'});
  cleanup.resolve(); await closing;
});

test('failed detector startup releases the transferred vault owner', async () => {
  const f = fixture();
  await assert.rejects(BrowserWorkspace.open({workspace: f.workspace, cnnEnabled: 'invalid'}), TypeError);
  assert.equal(f.workspace.closed, true);
});

const conversation = (value = 1) => ({id: new Uint8Array(16).fill(value), surface: 'synthetic-chat', siteId: null, siteIdByteBudget: 64});
function conversationPlan(effect = () => {}) {
  return {preview: () => ({text: 'protected'}), previews: () => [{text: 'protected'}], heads: () => ({}),
    sendIntents: () => [{preparationId: new Uint8Array(16).fill(9), inputIndex: 0}], discard() {}, effect};
}

test('conversation scans every source and snapshots route, manual selections, and operation before inference', async () => {
  const f = fixture(), first = deferred(), calls = [];
  let prepared;
  f.detector.scan = async (text, config) => {
    calls.push({text, config});
    if (calls.length === 1) return first.promise;
    return {findings: [{entityType: 'PERSON', matchedText: 'Bob', byteRange: {start: 0, end: 3}}]};
  };
  f.workspace.prepareConversationSend = (inputs, operation) => { prepared = {inputs, operation}; return conversationPlan(); };
  const inputs = [{input: {text: 'Alice', userSelections: [{entityType: 'PERSON', matchedText: 'Alice', range: {unit: 'utf8_bytes', start: 0, end: 5}}]}, conversation: conversation()},
    {input: {text: 'Bob'}, conversation: conversation(2)}];
  const operation = {id: new Uint8Array(16).fill(3), occurredAtMs: 1}, scan = {entities: ['PERSON']};
  const pending = f.owner.prepareConversationSend(inputs, operation, {scan});
  inputs[0].input.userSelections[0].matchedText = 'mutated'; inputs[0].conversation.id.fill(8);
  operation.id.fill(8); operation.occurredAtMs = 99; scan.entities[0] = 'EMAIL';
  assert.equal(prepared, undefined);
  first.resolve({findings: [{entityType: 'PERSON', matchedText: 'Alice', byteRange: {start: 0, end: 5}}]});
  const review = await pending;
  assert.deepEqual(calls.map(call => call.text), ['Alice', 'Bob']);
  assert.deepEqual(calls.map(call => call.config.entities), [['PERSON'], ['PERSON']]);
  assert.equal(prepared.inputs[0].input.userSelections[0].matchedText, 'Alice');
  assert.equal(prepared.inputs[0].conversation.id[0], 1);
  assert.equal(prepared.operation.id[0], 3); assert.equal(prepared.operation.occurredAtMs, 1);
  assert.deepEqual(prepared.inputs.map(entry => entry.input.scannerFindings[0].matchedText), ['Alice', 'Bob']);
  assert.equal(prepared.inputs[1].input.scannerFindings[0].range.unit, 'utf8_bytes');
  assert.equal(review.sendIntents()[0].inputIndex, 0);
  await f.owner.setCnnEnabled(false);
  assert.throws(() => review.sendIntents(), {code: 'stale_review'});
  await assert.rejects(f.owner.commit(review, {approved: true}), {code: 'stale_review'});
});

test('toggle between conversation scans rejects late findings without preparing a partial batch', async () => {
  const f = fixture(), second = deferred(), reachedSecond = deferred();
  let calls = 0, preparations = 0;
  f.detector.scan = async () => {
    calls++;
    if (calls === 2) { reachedSecond.resolve(); return second.promise; }
    return {findings: []};
  };
  f.workspace.prepareConversationSend = () => { preparations++; return conversationPlan(); };
  const pending = f.owner.prepareConversationSend([1, 2, 3].map(value => ({input: {text: 'source'}, conversation: conversation(value)})),
    {id: new Uint8Array(16), occurredAtMs: 1});
  await reachedSecond.promise;
  assert.throws(() => f.owner.prepareObservedSend({}, new Uint8Array(16), 'surface', 'site', {}), {code: 'workspace_busy'});
  await f.owner.setCnnEnabled(false);
  second.resolve({findings: []});
  await assert.rejects(pending, {code: 'stale_review'});
  assert.equal(calls, 2); assert.equal(preparations, 0);
});

test('conversation observations require approved persistence and scoped reads retain the owned flow', async () => {
  const f = fixture(), logicalId = new Uint8Array(16).fill(1), intent = {preparationId: new Uint8Array(16).fill(2), inputIndex: 0};
  let status = 'pending', observationArgs;
  f.workspace.prepareObservedSend = (...args) => { observationArgs = args; return conversationPlan(() => {status = 'observed_sent';}); };
  f.workspace.prepareDefinitelyNotSent = () => conversationPlan(() => {status = 'definitely_not_sent';});
  f.workspace.commit = async plan => { plan.effect(); return {previews: [], head: {}, recordRevision: 1n}; };
  f.workspace.sendIntentStatus = () => status;
  f.workspace.conversation = id => ({conversation: {id}, pendingIntents: status === 'pending' ? 1 : 0});
  f.workspace.conversationForSite = (surface, siteId) => ({surface, siteId});
  f.workspace.restoreInConversation = () => { if (status === 'pending') throw Object.assign(new Error(), {code: 'pending_send'}); return 'Alice Smith'; };
  f.workspace.restoreReportInConversation = () => ({text: status === 'pending' ? null : 'Alice Smith', issues: []});
  const review = f.owner.prepareObservedSend(intent, logicalId, 'surface', 'site', {id: new Uint8Array(16).fill(3), occurredAtMs: 2});
  intent.preparationId.fill(9); logicalId.fill(9);
  assert.equal(observationArgs[0].preparationId[0], 2); assert.equal(observationArgs[1][0], 1);
  assert.equal(f.owner.sendIntentStatus(intent), 'pending');
  assert.throws(() => f.owner.restoreInConversation('protected', logicalId), {code: 'pending_send'});
  assert.equal(f.owner.restoreReportInConversation('protected', logicalId).text, null);
  await assert.rejects(f.owner.commit(review, {approved: false}), {code: 'approval_required'});
  assert.equal(status, 'pending');
  await f.owner.commit(review, {approved: true});
  assert.equal(f.owner.sendIntentStatus(intent), 'observed_sent');
  assert.equal(f.owner.restoreInConversation('protected', logicalId), 'Alice Smith');
  assert.equal(f.owner.conversation(logicalId).pendingIntents, 0);
  assert.deepEqual(f.owner.conversationForSite('surface', 'site'), {surface: 'surface', siteId: 'site'});
  const cancelled = f.owner.prepareDefinitelyNotSent(intent, logicalId, {id: new Uint8Array(16).fill(4), occurredAtMs: 3});
  assert.equal(status, 'observed_sent');
  await f.owner.commit(cancelled, {approved: true});
  assert.equal(status, 'definitely_not_sent');
  await f.owner.close();
  assert.throws(() => f.owner.sendIntentStatus(intent), {code: 'workspace_closed'});
  assert.throws(() => f.owner.restoreInConversation('protected', logicalId), {code: 'workspace_closed'});
});
