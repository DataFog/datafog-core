import { HostError } from "./rpc.js";
import { equalBytes } from "./storage.js";

export const randomId = () => crypto.getRandomValues(new Uint8Array(16));
const random = length => crypto.getRandomValues(new Uint8Array(length));
const encoder = new TextEncoder();
const MAX_BUNDLE = 16 * 1024 * 1024;
function bytes(value, length) { if (!(value instanceof Uint8Array) || value.length !== length) throw new HostError("invalid_envelope"); }
function fields(value, keys) {
  if (!value || typeof value !== "object" || Object.keys(value).sort().join(",") !== [...keys].sort().join(",")) throw new HostError("invalid_envelope");
}
function concat(...arrays) {
  const result = new Uint8Array(arrays.reduce((n, a) => n + a.length, 0));
  let offset = 0; for (const array of arrays) { result.set(array, offset); offset += array.length; }
  return result;
}
function revisionBytes(head) {
  fields(head, ["revision", "commitId"]);
  if (typeof head.revision !== "string" || !/^(0|[1-9][0-9]{0,19})$/.test(head.revision) || !Array.isArray(head.commitId) || head.commitId.length !== 16 || !Array.from({ length: 16 }, (_, i) => Number.isInteger(head.commitId[i]) && head.commitId[i] >= 0 && head.commitId[i] <= 255).every(Boolean)) throw new HostError("invalid_envelope");
  const revision = BigInt(head.revision);
  if (revision > 18446744073709551615n) throw new HostError("invalid_envelope");
  const array = new Uint8Array(8); new DataView(array.buffer).setBigUint64(0, revision, false);
  return array;
}
function passwordAAD(vault) { return concat(encoder.encode("DFG1:PW:"), vault.vaultId, vault.vaultKeyId, new Uint8Array([vault.profile]), vault.salt); }
function keyAAD(record) { return concat(encoder.encode("DFG1:WK:"), record.vaultId, record.workspaceId, record.keyId); }
function snapshotAAD(record) { return concat(encoder.encode("DFG1:SN:"), record.vaultId, record.workspaceId, record.keyId, revisionBytes(record.head), new Uint8Array(record.head.commitId)); }
async function importKey(raw) { return crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["encrypt", "decrypt"]); }
async function encrypt(key, iv, aad, plaintext) { return new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv, additionalData: aad, tagLength: 128 }, key, plaintext)); }
async function decrypt(key, iv, aad, ciphertext) { return new Uint8Array(await crypto.subtle.decrypt({ name: "AES-GCM", iv, additionalData: aad, tagLength: 128 }, key, ciphertext)); }
export function validateVault(vault) {
  fields(vault, ["version", "vaultId", "vaultKeyId", "workspaceId", "profile", "salt", "rootNonce", "wrappedRoot", "accessGeneration"]);
  if (vault.version !== 1 || vault.profile !== 1) throw new HostError("invalid_envelope");
  for (const name of ["vaultId", "vaultKeyId", "workspaceId", "accessGeneration", "salt"]) bytes(vault[name], 16);
  bytes(vault.rootNonce, 12); bytes(vault.wrappedRoot, 48);
}
function validateRecord(record, vault) {
  fields(record, ["version", "vaultId", "workspaceId", "keyId", "head", "keyNonce", "wrappedKey", "nonce", "ciphertext"]);
  if (record.version !== 1) throw new HostError("invalid_envelope");
  for (const name of ["vaultId", "workspaceId", "keyId"]) bytes(record[name], 16);
  if (!equalBytes(record.vaultId, vault.vaultId) || !equalBytes(record.workspaceId, vault.workspaceId)) throw new HostError("invalid_envelope");
  bytes(record.keyNonce, 12); bytes(record.wrappedKey, 48); bytes(record.nonce, 12);
  if (!(record.ciphertext instanceof Uint8Array) || record.ciphertext.length < 16 || record.ciphertext.length > MAX_BUNDLE + 16) throw new HostError("invalid_envelope");
  revisionBytes(record.head);
}

function derive(password, salt, signal) {
  if (typeof password !== "string" || !password.isWellFormed() || password.length === 0 || password.length > 1024) return Promise.reject(new HostError("invalid_credentials"));
  const worker = new Worker(new URL("./kdf-worker.js", import.meta.url), { type: "module" });
  return new Promise((resolve, reject) => {
    const stop = () => { worker.terminate(); signal?.removeEventListener("abort", cancel); };
    const cancel = () => { stop(); reject(new HostError("cancelled")); };
    worker.onmessage = ({ data }) => { stop(); if (data.key instanceof Uint8Array && data.key.length === 32) resolve(data.key); else reject(new HostError("invalid_credentials")); };
    worker.onerror = () => { stop(); reject(new HostError("invalid_credentials")); };
    signal?.addEventListener("abort", cancel, { once: true });
    if (signal?.aborted) { cancel(); return; }
    worker.postMessage({ password, salt });
  });
}

export async function createVault(password, workspaceId, bundle, head, signal) {
  const vault = { version: 1, vaultId: randomId(), vaultKeyId: randomId(), workspaceId, profile: 1, salt: random(16), rootNonce: random(12), accessGeneration: randomId() };
  const root = random(32), data = random(32);
  let derived;
  try {
    derived = await derive(password, vault.salt, signal);
    vault.wrappedRoot = await encrypt(await importKey(derived), vault.rootNonce, passwordAAD(vault), root);
    const rootKey = await importKey(root);
    const dataKey = await importKey(data);
    const record = { version: 1, vaultId: vault.vaultId, workspaceId, keyId: randomId(), head, keyNonce: random(12), nonce: random(12) };
    record.wrappedKey = await encrypt(rootKey, record.keyNonce, keyAAD(record), data);
    record.ciphertext = await encrypt(dataKey, record.nonce, snapshotAAD(record), bundle);
    return { vault, record, dataKey };
  } finally { root.fill(0); data.fill(0); derived?.fill(0); bundle.fill(0); }
}
export async function openVault(password, vault, record, signal) {
  validateVault(vault); validateRecord(record, vault);
  let derived, root, data, bundle;
  try {
    derived = await derive(password, vault.salt, signal);
    root = await decrypt(await importKey(derived), vault.rootNonce, passwordAAD(vault), vault.wrappedRoot);
    data = await decrypt(await importKey(root), record.keyNonce, keyAAD(record), record.wrappedKey);
    const dataKey = await importKey(data);
    bundle = await decrypt(dataKey, record.nonce, snapshotAAD(record), record.ciphertext);
    return { dataKey, bundle };
  } catch (error) {
    bundle?.fill(0);
    if (error instanceof HostError) throw error;
    throw new HostError("invalid_credentials");
  } finally { derived?.fill(0); root?.fill(0); data?.fill(0); }
}
export async function encryptCandidate(dataKey, record, head, bundle) {
  if (bundle.length > MAX_BUNDLE) throw new HostError("resource_limit");
  const next = { ...record, head, nonce: random(12) };
  try { next.ciphertext = await encrypt(dataKey, next.nonce, snapshotAAD(next), bundle); return next; }
  finally { bundle.fill(0); }
}
export async function decryptCandidate(dataKey, record) {
  try { return await decrypt(dataKey, record.nonce, snapshotAAD(record), record.ciphertext); }
  catch { throw new HostError("invalid_envelope"); }
}
