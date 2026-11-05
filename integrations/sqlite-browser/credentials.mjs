const ITERATIONS = 600000;
const encoder = new TextEncoder();
const fail = () => { throw Object.assign(new Error('Invalid vault credentials'), {code: 'invalid_credentials'}); };
const random = length => crypto.getRandomValues(new Uint8Array(length));
const context = id => encoder.encode(`datafog-sqlite-credentials-v1:${id}`);
async function wrappingKey(secret, salt, recovery) {
  if (recovery) {
    if (!(secret instanceof Uint8Array) || secret.length !== 32) fail();
    return crypto.subtle.importKey('raw', secret, 'AES-GCM', false, ['encrypt', 'decrypt']);
  }
  if (typeof secret !== 'string' || secret.length < 12 || encoder.encode(secret).length > 1024) fail();
  const bytes = encoder.encode(secret);
  try {
    const base = await crypto.subtle.importKey('raw', bytes, 'PBKDF2', false, ['deriveKey']);
    return crypto.subtle.deriveKey({name: 'PBKDF2', hash: 'SHA-256', salt, iterations: ITERATIONS}, base,
      {name: 'AES-GCM', length: 256}, false, ['encrypt', 'decrypt']);
  } finally { bytes.fill(0); }
}
async function wrap(id, key, secret, recovery = false) {
  const salt = random(16), iv = random(12), wrapping = await wrappingKey(secret, salt, recovery);
  return {salt, iv, data: new Uint8Array(await crypto.subtle.encrypt({name: 'AES-GCM', iv, additionalData: context(id)}, wrapping, key))};
}
export async function makeEnvelope(id, key, password, recoveryKey, initialized) {
  const envelope = {format: 1, id, iterations: ITERATIONS, password: await wrap(id, key, password),
    ...(recoveryKey ? {recovery: await wrap(id, key, recoveryKey, true)} : {})};
  return initialized === undefined ? envelope : sealState(envelope, key, initialized);
}
export async function unlockEnvelope(id, envelope, password, recoveryKey) {
  if (!envelope || envelope.format !== 1 || envelope.id !== id || envelope.iterations !== ITERATIONS) fail();
  const recovery = recoveryKey !== undefined, entry = recovery ? envelope.recovery : envelope.password;
  if (!entry || !(entry.salt instanceof Uint8Array) || entry.salt.length !== 16 ||
      !(entry.iv instanceof Uint8Array) || entry.iv.length !== 12 || !(entry.data instanceof Uint8Array) || entry.data.length !== 48) fail();
  try {
    const wrapping = await wrappingKey(recovery ? recoveryKey : password, entry.salt, recovery);
    const decoded = new Uint8Array(await crypto.subtle.decrypt({name: 'AES-GCM', iv: entry.iv, additionalData: context(id)}, wrapping, entry.data));
    if (envelope.initialized !== undefined) {
      try {
        if (typeof envelope.initialized !== 'boolean' || !(envelope.stateTag instanceof Uint8Array) || envelope.stateTag.length !== 32) fail();
        const stateKey = await crypto.subtle.importKey('raw', decoded, {name: 'HMAC', hash: 'SHA-256'}, false, ['verify']);
        if (!await crypto.subtle.verify('HMAC', stateKey, envelope.stateTag, stateContext(envelope))) fail();
      } catch { decoded.fill(0); fail(); }
    }
    return decoded;
  } catch { fail(); }
}
export async function envelopeStore(id, value, replace = false) {
  const db = await new Promise((resolve, reject) => {
    const request = indexedDB.open('datafog-sqlite-credentials-v1', 1);
    request.onupgradeneeded = () => request.result.createObjectStore('vaults');
    request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error);
  });
  try {
    return await new Promise((resolve, reject) => {
      const transaction = db.transaction('vaults', value ? 'readwrite' : 'readonly');
      const request = value ? transaction.objectStore('vaults')[replace ? 'put' : 'add'](value, id) : transaction.objectStore('vaults').get(id);
      let result;
      request.onsuccess = () => { result = request.result; };
      transaction.oncomplete = () => resolve(result);
      transaction.onabort = () => reject(Object.assign(new Error('Credential storage failed'), {code: 'credential_storage_failed'}));
      transaction.onerror = () => {};
    });
  } finally { db.close(); }
}

const archiveContext = id => encoder.encode(`datafog-sqlite-backup-v1:${id}`);
export async function encryptArchive(id, data, password) {
  const key = random(32), iv = random(12);
  try {
    const envelope = await makeEnvelope(id, key, password);
    const cryptoKey = await crypto.subtle.importKey('raw', key, 'AES-GCM', false, ['encrypt']);
    const ciphertext = new Uint8Array(await crypto.subtle.encrypt({name: 'AES-GCM', iv, additionalData: archiveContext(id)}, cryptoKey, data));
    const json = JSON.stringify({format: 'datafog-sqlite-backup-v1', id, envelope, iv, ciphertext}, (_, value) => value instanceof Uint8Array ? {$bytes: Array.from(value)} : value);
    return encoder.encode(json);
  } finally { key.fill(0); }
}
export async function decryptArchive(bytes, password) {
  if (!(bytes instanceof Uint8Array) || bytes.length > 128 * 1024 * 1024) fail();
  let archive;
  try {
    archive = JSON.parse(new TextDecoder('utf-8', {fatal: true}).decode(bytes), (_, value) => {
      if (value && Object.hasOwn(value, '$bytes')) {
        if (Object.keys(value).length !== 1 || !Array.isArray(value.$bytes) || !value.$bytes.every(x => Number.isInteger(x) && x >= 0 && x <= 255)) fail();
        return Uint8Array.from(value.$bytes);
      }
      return value;
    });
    if (archive.format !== 'datafog-sqlite-backup-v1' || !/^[a-f0-9]{32}$/.test(archive.id) || !(archive.iv instanceof Uint8Array) || archive.iv.length !== 12 || !(archive.ciphertext instanceof Uint8Array)) fail();
    const key = await unlockEnvelope(archive.id, archive.envelope, password);
    try {
      const cryptoKey = await crypto.subtle.importKey('raw', key, 'AES-GCM', false, ['decrypt']);
      return new Uint8Array(await crypto.subtle.decrypt({name: 'AES-GCM', iv: archive.iv, additionalData: archiveContext(archive.id)}, cryptoKey, archive.ciphertext));
    } finally { key.fill(0); }
  } catch { fail(); }
}

const stateContext = envelope => encoder.encode(`datafog-sqlite-state-v1:${envelope.id}:${envelope.initialized}`);
export async function sealState(envelope, key, initialized) {
  const value = {...envelope, initialized};
  const stateKey = await crypto.subtle.importKey('raw', key, {name: 'HMAC', hash: 'SHA-256'}, false, ['sign']);
  value.stateTag = new Uint8Array(await crypto.subtle.sign('HMAC', stateKey, stateContext(value)));
  return value;
}
