import initialize from './vendor/sqlite3.mjs';
import {makeEnvelope, unlockEnvelope, envelopeStore, encryptArchive, sealState} from './credentials.mjs';
let db, sqlite, vaultId, databaseKey, maxPayload, queue = Promise.resolve();
const errorCodes = new Set(['invalid_credentials', 'invalid_identity', 'resource_limit', 'occupied_destination', 'not_found', 'storage_unavailable', 'unsupported_cipher', 'binding_mismatch', 'unsupported_schema', 'already_open', 'owner_closed', 'invalid_operation', 'NotFound', 'conflict', 'request_reused', 'outcome_unknown', 'credential_storage_failed']);
const error = code => Object.assign(new Error(code), {code});
const hex = bytes => Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
const one = (sql, bind = []) => db.exec({sql, bind, rowMode: 'object', returnValue: 'resultRows'})[0];
const revision = () => BigInt(one('SELECT revision FROM vault WHERE id=1').revision);
function transaction(action) {
  db.exec('BEGIN IMMEDIATE');
  try { const result = action(); db.exec('COMMIT'); return result; }
  catch (failure) { try { db.exec('ROLLBACK'); } catch { throw error('outcome_unknown'); } throw failure; }
}
async function open(options) {
  if (db) throw error('already_open');
  const {id, password, recoveryKey, create, maxPayloadBytes} = options;
  if (typeof id !== 'string' || !/^[a-f0-9]{32}$/.test(id) || /^0+$/.test(id)) throw error('invalid_identity');
  if (!Number.isSafeInteger(maxPayloadBytes) || maxPayloadBytes < 1 || maxPayloadBytes > 16 * 1024 * 1024) throw error('resource_limit');
  vaultId = id; maxPayload = maxPayloadBytes;
  let envelope = await envelopeStore(id), key;
  if (create) {
    if (envelope) throw error('occupied_destination');
    key = crypto.getRandomValues(new Uint8Array(32));
    envelope = await makeEnvelope(id, key, password, recoveryKey, false);
    await envelopeStore(id, envelope);
  } else {
    if (!envelope) throw error('not_found');
    if (typeof envelope.initialized !== 'boolean') throw error('invalid_credentials');
    key = await unlockEnvelope(id, envelope, password, recoveryKey);
  }
  try {
    globalThis.sqlite3ApiConfig = {disable: {vfs: {opfs: true, 'opfs-vfs': true, 'opfs-wl': true, kvvfs: true}}};
    sqlite = await initialize({print() {}, printErr() {}});
    sqlite.config.error = sqlite.config.warn = sqlite.config.log = () => {};
    const pool = await sqlite.installOpfsSAHPoolVfs({name: 'dfsql', directory: `.datafog-sqlite-${id}`, initialCapacity: 6, preserveOnInitFailure: true, verbosity: 0});
    if (sqlite.capi.sqlite3mc_vfs_create(pool.vfsName, 0) !== 0) throw error('storage_unavailable');
    db = new sqlite.oo1.DB({filename: '/vault.db', flags: create || envelope.initialized === false ? 'c' : 'w', vfs: `multipleciphers-${pool.vfsName}`});
    const cipher = sqlite.capi.sqlite3mc_cipher_index('chacha20');
    if (cipher <= 0 || sqlite.capi.sqlite3mc_config(db.pointer, 'cipher', cipher) !== cipher) throw error('unsupported_cipher');
    const pointer = sqlite.wasm.allocFromTypedArray(key);
    try { if (sqlite.capi.sqlite3_key_v2(db.pointer, 'main', pointer, key.length) !== 0) throw error('invalid_credentials'); }
    finally { sqlite.wasm.heap8u().fill(0, pointer, pointer + key.length); sqlite.wasm.dealloc(pointer); }
    db.exec('PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA temp_store=MEMORY; PRAGMA secure_delete=ON; PRAGMA max_page_count=65536;');
    const tables = one("SELECT count(*) AS count FROM sqlite_master WHERE type='table'").count;
    if (tables === 0 && envelope.initialized === false) transaction(() => {
      db.exec('CREATE TABLE vault(id INTEGER PRIMARY KEY CHECK(id=1), identity TEXT NOT NULL, revision TEXT NOT NULL); CREATE TABLE records(id TEXT PRIMARY KEY, revision TEXT NOT NULL, metadata BLOB NOT NULL, payload BLOB NOT NULL, request TEXT NOT NULL UNIQUE);');
      db.exec({sql: 'INSERT INTO vault VALUES(1,?,?)', bind: [id, '0']});
      db.exec('PRAGMA user_version=1');
    });
    if (db.selectValue('PRAGMA user_version') !== 1) throw error('unsupported_schema');
    if (one('SELECT identity FROM vault WHERE id=1')?.identity !== id) throw error('binding_mismatch');
    if (envelope.initialized === false) await envelopeStore(id, await sealState(envelope, key, true), true);
    databaseKey = key.slice();
    return {revision: revision(), engine: sqlite.capi.sqlite3mc_version(), vfs: pool.vfsName, cipher: 'chacha20-poly1305'};
  } finally { key?.fill(0); }
}
const methods = {
  open,
  receipt() { return {revision: revision()}; },
  read(id) {
    const size = one('SELECT length(payload) AS payload, length(metadata) AS metadata FROM records WHERE id=?', [hex(id)]);
    if (size && (size.payload > maxPayload || size.metadata !== 88)) throw error('resource_limit');
    const row = one('SELECT * FROM records WHERE id=?', [hex(id)]);
    if (!row) throw error('NotFound');
    return {revision: BigInt(row.revision), metadata: row.metadata, payload: row.payload};
  },
  save({id, metadata, payload, expectedRevision, expectedVaultRevision, requestId}) {
    if (!(metadata instanceof Uint8Array) || metadata.length !== 88 || !(payload instanceof Uint8Array) || payload.length > maxPayload) throw error('resource_limit');
    return transaction(() => {
      const current = revision(), row = one('SELECT revision FROM records WHERE id=?', [hex(id)]);
      if (!row && one('SELECT count(*) AS count FROM records').count >= 64) throw error('resource_limit');
      if (current !== expectedVaultRevision || BigInt(row?.revision || '0') !== expectedRevision) throw error('conflict');
      if (one('SELECT id FROM records WHERE request=?', [hex(requestId)])) throw error('request_reused');
      const next = current + 1n;
      db.exec({sql: 'INSERT INTO records VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision, metadata=excluded.metadata, payload=excluded.payload, request=excluded.request',
        bind: [hex(id), (expectedRevision + 1n).toString(), metadata, payload, hex(requestId)]});
      db.exec({sql: 'UPDATE vault SET revision=? WHERE id=1', bind: [next.toString()]});
      return {saved: true, requestId, receipt: {revision: next}};
    });
  },
  inventory() { return {records: db.exec({sql: 'SELECT id FROM records LIMIT 1', rowMode: 'object', returnValue: 'resultRows'}), complete: true, receipt: {revision: revision()}}; },
  reconcileLatest(requestId) { return {revision: revision(), found: !!one('SELECT id FROM records WHERE request=?', [hex(requestId)])}; },
  async changePassword({password, recoveryKey}) {
    const envelope = await makeEnvelope(vaultId, databaseKey, password, recoveryKey, true);
    await envelopeStore(vaultId, envelope, true);
    return true;
  },
  async exportBackup({recordId, password}) {
    const row = methods.read(recordId);
    const data = new Uint8Array(88 + row.payload.length);
    data.set(row.metadata); data.set(row.payload, 88);
    try { return await encryptArchive(vaultId, data, password); }
    finally { data.fill(0); row.payload.fill(0); row.metadata.fill(0); }
  },
  close() { db?.close(); db = null; databaseKey?.fill(0); databaseKey = null; return true; },
};
self.onmessage = ({data}) => {
  queue = queue.then(async () => {
    try {
      if (data.method !== 'open' && !db) throw error('owner_closed');
      if (!Object.hasOwn(methods, data.method)) throw error('invalid_operation');
      const result = await methods[data.method](data.args);
      self.postMessage({id: data.id, result});
    } catch (failure) {
      self.postMessage({id: data.id, error: {code: errorCodes.has(failure?.code) ? failure.code : 'storage_failed'}});
    }
  });
};
