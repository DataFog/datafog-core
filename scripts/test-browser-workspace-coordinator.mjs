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
