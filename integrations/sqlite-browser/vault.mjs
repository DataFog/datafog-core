import {WorkspaceVaultError, bytes16} from './binding.mjs';
const claim = Symbol('transfer vault ownership');
export const claimVault = vault => {
  if (!(vault instanceof SQLiteVault)) throw new WorkspaceVaultError('invalid_owner');
  return vault[claim]();
};
export class SQLiteVault {
  #worker; #pending = new Map(); #next = 0; #closed = false; #epoch = 0; #claimed = false;
  constructor(workerUrl) {
    this.#worker = new Worker(workerUrl || new URL('./worker.mjs', import.meta.url), {type: 'module'});
    this.#worker.onmessage = ({data}) => {
      const pending = this.#pending.get(data.id);
      if (!pending) return;
      this.#pending.delete(data.id);
      if (data.error) pending.reject(new WorkspaceVaultError(data.error.code)); else pending.resolve(data.result);
    };
    this.#worker.onerror = () => this.#revoke('outcome_unknown');
  }
  static async create(options) { return this.#open(options, true); }
  static async open(options) { return this.#open(options, false); }
  static async #open({id, password, recoveryKey, workerUrl, maxPayloadBytes = 16 * 1024 * 1024}, create) {
    const owner = new SQLiteVault(workerUrl);
    try {
      await owner.#call('open', {id, password, recoveryKey: recoveryKey?.slice(), maxPayloadBytes, create});
      return owner;
    } catch (failure) { owner.#revoke('owner_closed'); throw failure; }
  }
  #call(method, args) {
    if (this.#closed) return Promise.reject(new WorkspaceVaultError('owner_closed'));
    const id = ++this.#next;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => this.#revoke('outcome_unknown'), 120000);
      this.#pending.set(id, {resolve: value => {clearTimeout(timer); resolve(value);}, reject: failure => {clearTimeout(timer); reject(failure);}});
      try { this.#worker.postMessage({id, method, args}); }
      catch { this.#pending.get(id)?.reject(new WorkspaceVaultError('storage_failed')); this.#pending.delete(id); }
    });
  }
  #revoke(code) {
    this.#closed = true; this.#epoch++; this.#worker.terminate();
    for (const pending of this.#pending.values()) pending.reject(new WorkspaceVaultError(code));
    this.#pending.clear();
  }
  get locked() { return this.#closed; }
  get closed() { return this.#closed; }
  changePassword({password, recoveryKey}) { this.#public(); return this.#call('changePassword', {password, recoveryKey: recoveryKey?.slice()}); }
  receipt() { this.#public(); return this.#call('receipt'); }
  read(id) { this.#public(); return this.#call('read', bytes16(id)); }
  save(id, metadata, payload, options) {
    this.#public(); return this.#call('save', {id: bytes16(id), metadata: metadata.slice(), payload: payload.slice(), ...options,
      requestId: bytes16(options.requestId)});
  }
  async beginRecords() { this.#public(); const value = await this.#call('inventory'); return {receipt: value.receipt}; }
  recordPage() { this.#public(); return this.#call('inventory'); }
  async cancelRecords() { this.#public(); }
  reconcileLatest(requestId) { this.#public(); return this.#call('reconcileLatest', bytes16(requestId)); }
  lock() { this.#public(); this.#revoke('outcome_unknown'); return Promise.resolve(); }
  close() { return this.lock(); }
  #public() { if (this.#claimed) throw new WorkspaceVaultError('ownership_transferred'); }
  [claim]() {
    this.#public(); this.#claimed = true;
    const owner = this;
    return Object.freeze({
      get locked() { return owner.locked; }, get closed() { return owner.closed; },
      receipt: () => owner.#call('receipt'), read: id => owner.#call('read', bytes16(id)),
      save: (id, metadata, payload, options) => owner.#call('save', {id: bytes16(id), metadata: metadata.slice(), payload: payload.slice(),
        ...options, requestId: bytes16(options.requestId)}),
      changePassword: ({password, recoveryKey}) => owner.#call('changePassword', {password, recoveryKey: recoveryKey?.slice()}),
      exportBackup: (recordId, password) => owner.#call('exportBackup', {recordId: bytes16(recordId), password}),
      beginRecords: async () => { const value = await owner.#call('inventory'); return {receipt: value.receipt}; },
      recordPage: () => owner.#call('inventory'), cancelRecords: async () => {},
      lock: async () => owner.#revoke('outcome_unknown'), close: async () => owner.#revoke('outcome_unknown'),
    });
  }
}
