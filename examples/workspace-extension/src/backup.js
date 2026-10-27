import {HostError} from "./rpc.js";
import {validateVault, validateRecord, openVault, randomId, toBase64url, fromBase64url} from "./vault.js";

export const MAX_BACKUP_BYTES = 24 * 1024 * 1024;
const encoder = new TextEncoder();
const vaultFields = ["vaultId", "vaultKeyId", "workspaceId", "salt", "rootNonce", "wrappedRoot"];
const recordFields = ["vaultId", "workspaceId", "keyId", "keyNonce", "wrappedKey", "nonce", "ciphertext"];
function fields(value, expected) {
  if (!value || typeof value !== "object" || Object.keys(value).sort().join(",") !== [...expected].sort().join(",")) throw new HostError("invalid_backup");
}
function payload(vault, record, exportedAt) {
  return {format: "datafog-workspace-backup", version: 1, exportedAt,
    vault: {version: vault.version, profile: vault.profile, ...Object.fromEntries(vaultFields.map(key => [key, toBase64url(vault[key])])),
      recovery: {version: vault.recovery.version, salt: toBase64url(vault.recovery.salt), nonce: toBase64url(vault.recovery.nonce), wrappedRoot: toBase64url(vault.recovery.wrappedRoot)}},
    workspace: {version: record.version, head: {revision: record.head.revision, commitId: toBase64url(new Uint8Array(record.head.commitId))},
      ...Object.fromEntries(recordFields.map(key => [key, toBase64url(record[key])]))}};
}
function authentication(body, nonce) {
  return {name: "AES-GCM", iv: nonce, tagLength: 128,
    additionalData: encoder.encode(`DFGB1:AUTH:${JSON.stringify(body)}`)};
}

/** Export committed ciphertext and credential wrappers; no credentials or runtime keys. */
export async function exportBackup(vault, record, dataKey) {
  validateVault(vault); validateRecord(record, vault);
  if (vault.version !== 2) throw new HostError("recovery_unavailable");
  const body = payload(vault, record, new Date().toISOString());
  const nonce = crypto.getRandomValues(new Uint8Array(12));
  const tag = new Uint8Array(await crypto.subtle.encrypt(authentication(body, nonce), dataKey, new Uint8Array()));
  const result = encoder.encode(JSON.stringify({payload: body, nonce: toBase64url(nonce), tag: toBase64url(tag)}));
  if (result.length > MAX_BACKUP_BYTES) throw new HostError("resource_limit");
  return result;
}

function decodeBackup(bytes) {
  if (!(bytes instanceof Uint8Array)) throw new HostError("invalid_backup");
  if (bytes.length > MAX_BACKUP_BYTES) throw new HostError("resource_limit");
  try {
    const source = new TextDecoder("utf-8", {fatal: true, ignoreBOM: true}).decode(bytes);
    const file = JSON.parse(source);
    fields(file, ["payload", "nonce", "tag"]);
    const body = file.payload;
    fields(body, ["format", "version", "exportedAt", "vault", "workspace"]);
    if (body.format !== "datafog-workspace-backup" || body.version !== 1 || typeof body.exportedAt !== "string" || body.exportedAt.length !== 24 || new Date(body.exportedAt).toISOString() !== body.exportedAt) throw new HostError("invalid_backup");
    const v = body.vault, r = body.workspace;
    fields(v, ["version", "profile", ...vaultFields, "recovery"]);
    fields(v.recovery, ["version", "salt", "nonce", "wrappedRoot"]);
    fields(r, ["version", "head", ...recordFields]); fields(r.head, ["revision", "commitId"]);
    if (v.version !== 2) throw new HostError("invalid_backup");
    const vaultSizes = {vaultId: 16, vaultKeyId: 16, workspaceId: 16, salt: 16, rootNonce: 12, wrappedRoot: 48};
    const recordSizes = {vaultId: 16, workspaceId: 16, keyId: 16, keyNonce: 12, wrappedKey: 48, nonce: 12, ciphertext: 16 * 1024 * 1024 + 16};
    const vault = {version: v.version, profile: v.profile, accessGeneration: randomId(),
      ...Object.fromEntries(vaultFields.map(key => [key, fromBase64url(v[key], vaultSizes[key])])),
      recovery: {version: v.recovery.version, salt: fromBase64url(v.recovery.salt, 16), nonce: fromBase64url(v.recovery.nonce, 12), wrappedRoot: fromBase64url(v.recovery.wrappedRoot, 48)}};
    const record = {version: r.version, head: {revision: r.head.revision, commitId: Array.from(fromBase64url(r.head.commitId, 16))},
      ...Object.fromEntries(recordFields.map(key => [key, fromBase64url(r[key], recordSizes[key])]))};
    validateVault(vault); validateRecord(record, vault);
    const nonce = fromBase64url(file.nonce, 12), tag = fromBase64url(file.tag, 16);
    if (nonce.length !== 12 || tag.length !== 16 || JSON.stringify({payload: payload(vault, record, body.exportedAt), nonce: toBase64url(nonce), tag: toBase64url(tag)}) !== source) throw new HostError("invalid_backup");
    return {vault, record, exportedAt: body.exportedAt, body, nonce, tag};
  } catch { throw new HostError("invalid_backup"); }
}

/** Authenticate every exported field, even the unused credential slot, before staging. */
export async function openBackup(bytes, credential, method, signal) {
  const decoded = decodeBackup(bytes);
  const opened = await openVault(credential, decoded.vault, decoded.record, signal, method);
  try {
    await crypto.subtle.decrypt(authentication(decoded.body, decoded.nonce), opened.dataKey, decoded.tag);
    return {vault: decoded.vault, record: decoded.record, exportedAt: decoded.exportedAt, ...opened};
  } catch {
    opened.bundle.fill(0);
    throw new HostError("invalid_backup");
  }
}
