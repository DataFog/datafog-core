import {CnnSession} from './index.js';

const reviews = new WeakMap();
const token = Symbol('browser-workspace');

export class BrowserWorkspaceError extends Error {
  constructor(code) { super(`Browser Workspace operation failed (${code})`); this.name = 'BrowserWorkspaceError'; this.code = code; }
}

class Review {
  constructor(owner, plan, generation) { reviews.set(this, {owner, plan, generation, active: true}); }
  preview() { return this.#current().plan.preview(); }
  previews() { return this.#current().plan.previews(); }
  heads() { return this.#current().plan.heads(); }
  #current() {
    const entry = reviews.get(this);
    if (!entry.active || entry.owner.closed || entry.generation !== entry.owner.generation) throw new BrowserWorkspaceError('stale_review');
    return entry;
  }
}

/** Owns the supplied detector and vault-backed Workspace exclusively. */
export class BrowserWorkspace {
  #workspace; #detector; #generation = 0; #closed = false; #busy = false; #saving = false; #pending; #closing;
  constructor(key, workspace, detector) {
    if (key !== token) throw new BrowserWorkspaceError('invalid_configuration');
    this.#workspace = workspace; this.#detector = detector;
  }
  /** Load the bundled detector before granting access to the owned Workspace. */
  static async open({workspace, ...detectorOptions}) {
    if (!workspace || workspace.closed || typeof workspace.close !== 'function') throw new BrowserWorkspaceError('invalid_configuration');
    let detector;
    try {
      detector = await CnnSession.open(detectorOptions);
      return BrowserWorkspace.adopt({workspace, detector});
    } catch (failure) {
      detector?.close();
      // Failed startup must release database ownership; cleanup failure propagates.
      await workspace.close();
      throw failure;
    }
  }
  get cnnEnabled() { return this.#detector.cnnEnabled; }
  get detectorStatus() { return this.#detector.status; }
  /** Transfer exclusive use of an already opened detector and vault owner. */
  static adopt({workspace, detector}) {
    if (!workspace || !detector || typeof workspace.prepareReview !== 'function' || typeof detector.scan !== 'function' ||
        typeof detector.setEnabled !== 'function' || typeof detector.close !== 'function' || workspace.closed) {
      throw new BrowserWorkspaceError('invalid_configuration');
    }
    return new BrowserWorkspace(token, workspace, detector);
  }
  get closed() { return this.#closed || this.#workspace.closed; }
  get generation() { return this.#generation; }
  #check() { if (this.closed) throw new BrowserWorkspaceError('workspace_closed'); }
  #invalidate() {
    this.#generation++;
    if (this.#pending) {
      const entry = reviews.get(this.#pending); entry.active = false;
      this.#pending = undefined;
      if (!this.#saving && !this.#workspace.closed) entry.plan.discard();
    }
  }
  async prepare(text, operationId, {scan, userSelections = []} = {}) {
    this.#check();
    if (this.#busy) throw new BrowserWorkspaceError('workspace_busy');
    const operation = operationId.slice(), selections = structuredClone(userSelections), config = structuredClone(scan);
    this.#invalidate(); this.#check();
    const generation = this.#generation; this.#busy = true;
    try {
      const result = await this.#detector.scan(text, config);
      this.#check();
      if (generation !== this.#generation) throw new BrowserWorkspaceError('stale_review');
      const scannerFindings = result.findings.map(finding => ({
        entityType: finding.entityType, matchedText: finding.matchedText,
        range: {unit: 'utf8_bytes', ...finding.byteRange}, detectorName: finding.detectorName,
        detectorVersion: finding.detectorVersion, source: 'scanner', needsReview: true,
        ...(finding.confidence == null ? {} : {confidence: finding.confidence}),
      }));
      const plan = this.#workspace.prepareReview([{text, scannerFindings, userSelections: selections}], operation);
      const review = new Review(this, plan, generation); this.#pending = review;
      return review;
    } finally { this.#busy = false; }
  }
  async commit(review, {approved, requestId} = {}) {
    this.#check();
    if (this.#busy) throw new BrowserWorkspaceError('workspace_busy');
    const entry = reviews.get(review);
    if (!entry || entry.owner !== this || !entry.active || review !== this.#pending || entry.generation !== this.#generation) {
      throw new BrowserWorkspaceError('stale_review');
    }
    if (approved !== true) throw new BrowserWorkspaceError('approval_required');
    const generation = this.#generation;
    const approval = {approved: true, ...(requestId === undefined ? {} : {requestId: requestId.slice()})};
    this.#busy = true; this.#saving = true;
    try {
      const result = await this.#workspace.commit(entry.plan, approval);
      this.#check();
      if (generation !== this.#generation) throw new BrowserWorkspaceError('stale_review');
      entry.active = false; this.#pending = undefined;
      return result;
    } finally { this.#busy = false; this.#saving = false; }
  }
  async setCnnEnabled(enabled) {
    this.#check();
    if (typeof enabled !== 'boolean') throw new BrowserWorkspaceError('invalid_configuration');
    this.#invalidate();
    // Revoke the vault owner before waiting for an already submitted save.
    if (this.#saving) { await this.close(); throw new BrowserWorkspaceError('reopen_required'); }
    await this.#detector.setEnabled(enabled);
    this.#check();
  }
  restore(text) { this.#check(); return this.#workspace.restore(text); }
  restoreReport(text) { this.#check(); return this.#workspace.restoreReport(text); }
  head() { this.#check(); return this.#workspace.head(); }
  close() {
    if (this.#closing) return this.#closing;
    this.#closed = true;
    this.#closing = this.#finishClose();
    return this.#closing;
  }
  async #finishClose() {
    let invalidationError;
    try { this.#invalidate(); } catch (error) { invalidationError = error; }
    const cleanup = [];
    for (const owner of [this.#workspace, this.#detector]) {
      try { cleanup.push(owner.close()); } catch (error) { cleanup.push(Promise.reject(error)); }
    }
    const results = await Promise.allSettled(cleanup);
    if (invalidationError) throw invalidationError;
    const failed = results.find(result => result.status === 'rejected');
    if (failed) throw failed.reason;
  }
  async lock() { return this.close(); }
}
