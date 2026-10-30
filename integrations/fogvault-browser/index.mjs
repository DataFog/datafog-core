import {WorkspaceVaultError, bytes16, equalBytes, head, sameHead, encodeBinding, decodeBinding} from './binding.mjs';
export {WorkspaceVaultError} from './binding.mjs';

const constructorToken = Symbol('owned Core workspace');
const plans = new WeakMap();
const randomId = () => {
  let value;
  do { value = crypto.getRandomValues(new Uint8Array(16)); } while (!value.some(Boolean));
  return value;
};
const clear = record => { record?.metadata?.fill(0); record?.payload?.fill(0); };
function receipt(value) {
  if (!value || typeof value.generation !== 'bigint' || value.generation < 0n ||
      !(value.head_digest instanceof Uint8Array) || value.head_digest.length !== 32 ||
      !(value.request_id instanceof Uint8Array) || value.request_id.length !== 16) {
    throw new WorkspaceVaultError('invalid_receipt');
  }
  return {generation: value.generation, head_digest: value.head_digest.slice(), request_id: value.request_id.slice()};
}

/** Local preview access does not grant transport or save authority. */
class WorkspaceVaultPlan {
  constructor(token, owner, prepared, check) {
    if (token !== constructorToken) throw new WorkspaceVaultError('foreign_plan');
    plans.set(this, {owner, prepared, active: true, check});
  }
  #call(method) {
    const state = plans.get(this);
    state.check(this);
    return state.prepared[method]();
  }
  preview() { return this.#call('preview'); }
  previews() { return this.#call('previews'); }
  heads() { return this.#call('heads'); }
  snapshotFormat() { return this.#call('snapshotFormat'); }
  policyPreview() { return this.#call('policyPreview'); }
  sendIntents() { return this.#call('sendIntents'); }
  discard() {
    const state = plans.get(this);
    state.check(this); state.prepared.discard(); state.active = false;
  }
}

/** Own both handles exclusively. Core is revoked synchronously before async vault cleanup. */
export class FogVaultWorkspace {
  #Workspace; #vault; #workspace; #recordId; #namespace; #recordRevision = 0n;
  #receipt; #closed = false; #busy = false; #epoch = 0;

  constructor(token, Workspace, vault, recordId, workspace) {
    if (token !== constructorToken) throw new WorkspaceVaultError('invalid_owner');
    this.#Workspace = Workspace; this.#vault = vault; this.#workspace = workspace;
    this.#recordId = bytes16(recordId); this.#namespace = bytes16(workspace.namespace());
  }

  static async create({Workspace, vault, recordId, namespace, operationId = randomId(), requestId = randomId()}) {
    const request = bytes16(requestId);
    bytes16(recordId); bytes16(namespace); bytes16(operationId);
    const workspace = Workspace.create(namespace, operationId);
    const owner = new FogVaultWorkspace(constructorToken, Workspace, vault, recordId, workspace);
    let payload;
    try {
      owner.#receipt = receipt(await vault.receipt()); owner.#checkEpoch(0);
      await owner.#requireVacant(0);
      payload = workspace.exportBundle();
      const initial = head(workspace.head());
      await owner.#save(payload, initial, initial, request, 0);
      return owner;
    } catch (error) { owner.#invalidate(); throw error; }
    finally { payload?.fill(0); }
  }

  static async open({Workspace, vault, recordId, expectedNamespace, expectedHead}) {
    const identity = bytes16(recordId);
    const wantedNamespace = expectedNamespace === undefined ? undefined : bytes16(expectedNamespace);
    const wantedHead = expectedHead === undefined ? undefined : head(expectedHead);
    let record, workspace;
    try {
      const before = receipt(await vault.receipt());
      record = await vault.read(identity);
      const after = receipt(await vault.receipt());
      if (!equalBytes(before.head_digest, after.head_digest) || before.generation !== after.generation) {
        throw new WorkspaceVaultError('reopen_required');
      }
      if (typeof record.revision !== 'bigint' || record.revision <= 0n || !(record.payload instanceof Uint8Array)) {
        throw new WorkspaceVaultError('invalid_record');
      }
      const binding = decodeBinding(record.metadata);
      if (!equalBytes(identity, binding.recordId) ||
          (wantedNamespace !== undefined && !equalBytes(wantedNamespace, binding.namespace)) ||
          (wantedHead !== undefined && !sameHead(wantedHead, binding.saved))) {
        throw new WorkspaceVaultError('binding_mismatch');
      }
      workspace = Workspace.openBundle(record.payload);
      if (!equalBytes(workspace.namespace(), binding.namespace) || !sameHead(workspace.head(), binding.saved)) {
        throw new WorkspaceVaultError('binding_mismatch');
      }
      const owner = new FogVaultWorkspace(constructorToken, Workspace, vault, identity, workspace);
      owner.#recordRevision = record.revision; owner.#receipt = after; owner.#checkEpoch(0);
      workspace = null;
      return owner;
    } finally { workspace?.close(); clear(record); }
  }

  /** Source authentication/decryption is a separate host duty; this validates exact trusted identity. */
  static async importAuthenticatedBundle({Workspace, vault, recordId, bundle, expectedNamespace, expectedHead,
    operationId = randomId(), requestId = randomId(), approved}) {
    if (approved !== true) throw new WorkspaceVaultError('approval_required');
    const request = bytes16(requestId);
    bytes16(recordId); bytes16(expectedNamespace); bytes16(operationId); head(expectedHead);
    if (!(bundle instanceof Uint8Array) || bundle.length > 16 * 1024 * 1024) throw new WorkspaceVaultError('invalid_record');
    let workspace, owner, payload;
    try {
      workspace = Workspace.openBundle(bundle);
      if (!equalBytes(workspace.namespace(), expectedNamespace) || !sameHead(workspace.head(), expectedHead)) {
        throw new WorkspaceVaultError('binding_mismatch');
      }
      owner = new FogVaultWorkspace(constructorToken, Workspace, vault, recordId, workspace);
      const recovery = workspace.prepareRecovery(operationId), heads = recovery.heads();
      payload = recovery.exportCandidateBundle();
      owner.#receipt = receipt(await vault.receipt()); owner.#checkEpoch(0);
      await owner.#requireEmptyVault(0);
      await owner.#requireVacant(0);
      await owner.#save(payload, heads.expectedHead, heads.candidateHead, request, 0);
      owner.#checkEpoch(0); workspace.activateSaved(recovery, heads.expectedHead, heads.candidateHead);
      owner.#checkEpoch(0); workspace = null;
      return owner;
    } catch (error) { owner?.#invalidate(); throw error; }
    finally { workspace?.close(); payload?.fill(0); }
  }

  #invalidate() {
    this.#epoch++; this.#closed = true;
    this.#workspace.close();
  }
  #checkEpoch(epoch) {
    if (this.#closed || epoch !== this.#epoch || this.#vault.locked || this.#vault.closed) {
      if (!this.#closed) this.#invalidate();
      throw new WorkspaceVaultError('owner_closed');
    }
  }
  #check() {
    this.#checkEpoch(this.#epoch);
    if (this.#busy) throw new WorkspaceVaultError('operation_in_progress');
  }
  #checkPlan(plan) {
    this.#check();
    const entry = plans.get(plan);
    if (!entry || entry.owner !== this || !entry.active) throw new WorkspaceVaultError('foreign_or_consumed_plan');
    return entry;
  }
  #prepare(method, args) {
    this.#check();
    return new WorkspaceVaultPlan(constructorToken, this, this.#workspace[method](...args), plan => this.#checkPlan(plan));
  }
  #read(method, args) { this.#check(); return this.#workspace[method](...args); }
  #stream(method, args) {
    this.#check();
    const stream = this.#workspace[method](...args), owner = this;
    return Object.freeze(Object.fromEntries(['feed', 'feedUtf16', 'finalize', 'reset', 'close'].map(name => [name, (...values) => {
      if (name !== 'close') owner.#check();
      return stream[name](...values);
    }])));
  }

  async #requireVacant(epoch) {
    let record;
    try {
      record = await this.#vault.read(this.#recordId); this.#checkEpoch(epoch);
      throw new WorkspaceVaultError('occupied_destination');
    } catch (error) {
      this.#checkEpoch(epoch);
      if (error?.code !== 'NotFound') throw error;
    } finally { clear(record); }
    const after = receipt(await this.#vault.receipt()); this.#checkEpoch(epoch);
    if (after.generation !== this.#receipt.generation || !equalBytes(after.head_digest, this.#receipt.head_digest)) {
      throw new WorkspaceVaultError('reopen_required');
    }
  }

  async #requireEmptyVault(epoch) {
    const begun = await this.#vault.beginRecords(); this.#checkEpoch(epoch);
    try {
      const captured = receipt(begun.receipt);
      if (captured.generation !== this.#receipt.generation || !equalBytes(captured.head_digest, this.#receipt.head_digest)) {
        throw new WorkspaceVaultError('reopen_required');
      }
      const page = await this.#vault.recordPage(1); this.#checkEpoch(epoch);
      const current = receipt(page.receipt);
      if (current.generation !== captured.generation || !equalBytes(current.head_digest, captured.head_digest)) {
        throw new WorkspaceVaultError('reopen_required');
      }
      if (!Array.isArray(page.records) || page.records.length !== 0 || page.complete !== true) {
        throw new WorkspaceVaultError('occupied_destination');
      }
    } finally { await this.#vault.cancelRecords(); this.#checkEpoch(epoch); }
  }

  async #save(payload, parent, saved, requestId, epoch) {
    const metadata = encodeBinding(this.#namespace, this.#recordId, parent, saved);
    try {
      const result = await this.#vault.save(this.#recordId, metadata, payload, {
        expectedRevision: this.#recordRevision, expectedHead: this.#receipt.head_digest.slice(), requestId,
      });
      this.#checkEpoch(epoch);
      const accepted = receipt(result.receipt);
      if (result.saved !== true || !equalBytes(accepted.request_id, requestId) || result.replay !== false ||
          accepted.generation !== this.#receipt.generation + 1n) {
        throw new WorkspaceVaultError('invalid_save_confirmation', requestId);
      }
      this.#receipt = accepted; this.#recordRevision++;
      return accepted;
    } finally { metadata.fill(0); }
  }

  /** Only this successful return authorizes release of its previews; it never sends them. */
  async commit(plan, {approved, requestId = randomId()} = {}) {
    const entry = this.#checkPlan(plan);
    if (approved !== true) throw new WorkspaceVaultError('approval_required');
    const request = bytes16(requestId), epoch = this.#epoch;
    const heads = entry.prepared.heads();
    const expected = head(heads.expectedHead), saved = head(heads.candidateHead);
    if (!sameHead(this.#workspace.head(), expected)) throw new WorkspaceVaultError('stale_plan');
    const previews = entry.prepared.previews();
    let payload, candidate;
    this.#busy = true;
    try {
      payload = entry.prepared.exportCandidateBundle();
      // Reject mismatched candidate identity/head before durable mutation.
      candidate = this.#Workspace.openBundle(payload);
      if (!equalBytes(candidate.namespace(), this.#namespace) || !sameHead(candidate.head(), saved)) {
        throw new WorkspaceVaultError('binding_mismatch');
      }
      candidate.close(); candidate = null;
      const accepted = await this.#save(payload, expected, saved, request, epoch);
      this.#checkEpoch(epoch);
      this.#workspace.activateSaved(entry.prepared, expected, saved);
      entry.active = false;
      this.#checkEpoch(epoch);
      return {previews, head: head(saved), recordRevision: this.#recordRevision, receipt: accepted};
    } catch (error) {
      this.#invalidate();
      if (error?.code === 'OutcomeUnknown') throw new WorkspaceVaultError('outcome_unknown', error.requestId || request);
      throw error;
    } finally { candidate?.close(); payload?.fill(0); this.#busy = false; }
  }

  get closed() { return this.#closed; }
  head() { return this.#read('head', []); }
  namespace() { return this.#read('namespace', []); }
  recordRevision() { this.#check(); return this.#recordRevision; }
  policy() { return this.#read('policy', []); }
  recall(text) { return this.#read('recall', [text]); }
  review(input) { return this.#read('review', [input]); }
  operations() { return this.#read('operations', []); }
  operationsPage(offset, limit) { return this.#read('operationsPage', [offset, limit]); }
  counts() { return this.#read('counts', []); }
  historyCoverage() { return this.#read('historyCoverage', []); }
  conversation(id) { return this.#read('conversation', [id]); }
  conversationForSite(surface, site) { return this.#read('conversationForSite', [surface, site]); }
  sendIntentStatus(intent) { return this.#read('sendIntentStatus', [intent]); }
  restore(text) { return this.#read('restore', [text]); }
  restoreReport(text) { return this.#read('restoreReport', [text]); }
  restoreInConversation(text, id) { return this.#read('restoreInConversation', [text, id]); }
  restoreReportInConversation(text, id) { return this.#read('restoreReportInConversation', [text, id]); }
  bufferedRestore(...args) { return this.#stream('bufferedRestore', args); }
  progressiveRestore(...args) { return this.#stream('progressiveRestore', args); }
  bufferedRestoreInConversation(...args) { return this.#stream('bufferedRestoreInConversation', args); }
  progressiveRestoreInConversation(...args) { return this.#stream('progressiveRestoreInConversation', args); }
  prepare(...args) { return this.#prepare('prepare', args); }
  prepareBatch(...args) { return this.#prepare('prepareBatch', args); }
  prepareReview(...args) { return this.#prepare('prepareReview', args); }
  prepareConversationSend(...args) { return this.#prepare('prepareConversationSend', args); }
  prepareObservedSend(...args) { return this.#prepare('prepareObservedSend', args); }
  prepareDefinitelyNotSent(...args) { return this.#prepare('prepareDefinitelyNotSent', args); }
  preparePolicyChanges(...args) { return this.#prepare('preparePolicyChanges', args); }
  prepareOutcome(...args) { return this.#prepare('prepareOutcome', args); }
  prepareRecovery(...args) { return this.#prepare('prepareRecovery', args); }
  prepareWithOperation(...args) { return this.#prepare('prepareWithOperation', args); }
  prepareBatchWithOperation(...args) { return this.#prepare('prepareBatchWithOperation', args); }
  prepareRecoveryWithOperation(...args) { return this.#prepare('prepareRecoveryWithOperation', args); }

  lock() {
    if (!this.#closed) this.#invalidate();
    return this.#vault.lock();
  }
  close() {
    if (!this.#closed) this.#invalidate();
    return this.#vault.close();
  }
}
