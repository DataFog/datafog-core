import {LegacyImportError as HostError} from "./error.mjs";
const equalBytes = (a,b) => a.length===b.length && a.every((v,i)=>v===b[i]);

export const randomId = () => crypto.getRandomValues(new Uint8Array(16));
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
function passwordAAD(vault) { return concat(encoder.encode(`DFG${vault.version}:PW:`), vault.vaultId, vault.vaultKeyId, new Uint8Array([vault.profile]), vault.salt); }
function recoveryAAD(vault) { return concat(encoder.encode("DFG2:RC:"), vault.vaultId, vault.vaultKeyId, new Uint8Array([vault.recovery.version]), vault.recovery.salt); }
function keyAAD(record) { return concat(encoder.encode("DFG1:WK:"), record.vaultId, record.workspaceId, record.keyId); }
function snapshotAAD(record) { return concat(encoder.encode("DFG1:SN:"), record.vaultId, record.workspaceId, record.keyId, revisionBytes(record.head), new Uint8Array(record.head.commitId)); }
async function importKey(raw) { return crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["encrypt", "decrypt"]); }
async function decrypt(key, iv, aad, ciphertext) { return new Uint8Array(await crypto.subtle.decrypt({ name: "AES-GCM", iv, additionalData: aad, tagLength: 128 }, key, ciphertext)); }
export function validateVault(vault) {
  const names = ["version", "vaultId", "vaultKeyId", "workspaceId", "profile", "salt", "rootNonce", "wrappedRoot", "accessGeneration"];
  if (vault?.version === 2) names.push("recovery");
  fields(vault, names);
  if (![1, 2].includes(vault.version) || ![1, 2].includes(vault.profile) || (vault.version === 1 && vault.profile !== 1)) throw new HostError("invalid_envelope");
  for (const name of ["vaultId", "vaultKeyId", "workspaceId", "accessGeneration", "salt"]) bytes(vault[name], 16);
  bytes(vault.rootNonce, 12); bytes(vault.wrappedRoot, 48);
  if (vault.version === 2) {
    fields(vault.recovery, ["version", "salt", "nonce", "wrappedRoot"]);
    if (vault.recovery.version !== 1) throw new HostError("invalid_envelope");
    bytes(vault.recovery.salt, 16); bytes(vault.recovery.nonce, 12); bytes(vault.recovery.wrappedRoot, 48);
  }
}
export function validateRecord(record, vault) {
  fields(record, ["version", "vaultId", "workspaceId", "keyId", "head", "keyNonce", "wrappedKey", "nonce", "ciphertext"]);
  if (record.version !== 1) throw new HostError("invalid_envelope");
  for (const name of ["vaultId", "workspaceId", "keyId"]) bytes(record[name], 16);
  if (!equalBytes(record.vaultId, vault.vaultId) || !equalBytes(record.workspaceId, vault.workspaceId)) throw new HostError("invalid_envelope");
  bytes(record.keyNonce, 12); bytes(record.wrappedKey, 48); bytes(record.nonce, 12);
  if (!(record.ciphertext instanceof Uint8Array) || record.ciphertext.length < 16 || record.ciphertext.length > MAX_BUNDLE + 16) throw new HostError("invalid_envelope");
  revisionBytes(record.head);
}

export function toBase64url(value) {
  const chunks = [];
  for (let start = 0; start < value.length; start += 8192) chunks.push(String.fromCharCode(...value.subarray(start, start + 8192)));
  return btoa(chunks.join("")).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}
export function fromBase64url(value, maximum) {
  if (typeof value !== "string" || value.length > Math.ceil(maximum * 4 / 3) || !/^[A-Za-z0-9_-]*$/.test(value)) throw new HostError("invalid_envelope");
  let raw;
  try { raw = Uint8Array.from(atob(value.replaceAll("-", "+").replaceAll("_", "/")), char => char.charCodeAt(0)); }
  catch { throw new HostError("invalid_envelope"); }
  if (raw.length > maximum || toBase64url(raw) !== value) { raw.fill(0); throw new HostError("invalid_envelope"); }
  return raw;
}
async function recoveryKey(secret, vault) {
  const key = await crypto.subtle.importKey("raw", secret, "HKDF", false, ["deriveKey"]);
  return crypto.subtle.deriveKey({name: "HKDF", hash: "SHA-256", salt: vault.recovery.salt,
    info: concat(encoder.encode("DFG2:RECOVERY:HKDF:"), vault.vaultId, vault.vaultKeyId)}, key,
    {name: "AES-GCM", length: 256}, false, ["encrypt", "decrypt"]);
}
function recoverySecret(code) {
  if (typeof code !== "string" || !/^DFR1\.[A-Za-z0-9_-]{43}$/.test(code)) throw new HostError("invalid_credentials");
  try { const secret = fromBase64url(code.slice(5), 32); bytes(secret, 32); return secret; }
  catch { throw new HostError("invalid_credentials"); }
}

function derive(password, salt, signal, profile = 1) {
  const passwordBytes = encoder.encode(password);
  const valid = typeof password === "string" && password.isWellFormed() && passwordBytes.length > 0 && passwordBytes.length <= 1024;
  passwordBytes.fill(0);
  if (!valid) return Promise.reject(new HostError("invalid_credentials"));
  if (signal?.aborted) return Promise.reject(new HostError("cancelled"));
  let worker;
  try { worker = new Worker(new URL("./kdf-worker.mjs", import.meta.url), { type: "module" }); }
  catch { return Promise.reject(new HostError("kdf_unavailable")); }
  return new Promise((resolve, reject) => {
    const stop = () => { worker.terminate(); signal?.removeEventListener("abort", cancel); };
    const cancel = () => { stop(); reject(new HostError("cancelled")); };
    worker.onmessage = ({ data }) => { stop(); if (data.key instanceof Uint8Array && data.key.length === 32) resolve(data.key); else reject(new HostError(data.error === "kdf_unavailable" ? "kdf_unavailable" : "invalid_credentials")); };
    worker.onerror = () => { stop(); reject(new HostError("kdf_unavailable")); };
    signal?.addEventListener("abort", cancel, { once: true });
    if (signal?.aborted) { cancel(); return; }
    worker.postMessage({ password, salt, profile });
  });
}

export async function openVault(credential, vault, record, signal, method = "password") {
  validateVault(vault); validateRecord(record, vault);
  let derived, secret, root, data, bundle;
  try {
    if (method === "password") {
      derived = await derive(credential, vault.salt, signal, vault.profile);
      root = await decrypt(await importKey(derived), vault.rootNonce, passwordAAD(vault), vault.wrappedRoot);
    } else if (method === "recovery" && vault.version === 2) {
      secret = recoverySecret(credential);
      root = await decrypt(await recoveryKey(secret, vault), vault.recovery.nonce, recoveryAAD(vault), vault.recovery.wrappedRoot);
    } else throw new HostError("invalid_credentials");
    const rootKey = await importKey(root);
    data = await decrypt(rootKey, record.keyNonce, keyAAD(record), record.wrappedKey);
    const dataKey = await importKey(data);
    bundle = await decrypt(dataKey, record.nonce, snapshotAAD(record), record.ciphertext);
    return { dataKey, rootKey, bundle };
  } catch (error) {
    bundle?.fill(0);
    if (error instanceof HostError) throw error;
    throw new HostError("invalid_credentials");
  } finally { derived?.fill(0); secret?.fill(0); root?.fill(0); data?.fill(0); }
}
