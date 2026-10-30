const DOMAIN = new TextEncoder().encode('DFVBRG01');
const U64_MAX = (1n << 64n) - 1n;

export class WorkspaceVaultError extends Error {
  constructor(code, requestId) {
    super(`Core–FogVault integration: ${code}`);
    this.name = 'WorkspaceVaultError';
    this.code = code;
    if (requestId instanceof Uint8Array && requestId.length === 16) this.requestId = requestId.slice();
  }
}

export function bytes16(value) {
  if (!(value instanceof Uint8Array) || value.length !== 16 || !value.some(Boolean)) {
    throw new WorkspaceVaultError('invalid_identity');
  }
  return value.slice();
}

export function equalBytes(a, b) {
  return a.length === b.length && a.every((value, index) => value === b[index]);
}

export function head(value) {
  if (!value || typeof value.revision !== 'string' || value.revision.length > 20 || !/^(0|[1-9][0-9]*)$/.test(value.revision)) {
    throw new WorkspaceVaultError('invalid_head');
  }
  const revision = BigInt(value.revision);
  const commit = value.commitId;
  if (revision > U64_MAX || !(commit instanceof Uint8Array || Array.isArray(commit)) ||
      commit.length !== 16 || !Array.from(commit).every(x => Number.isInteger(x) && x >= 0 && x <= 255)) {
    throw new WorkspaceVaultError('invalid_head');
  }
  return {revision: revision.toString(), commitId: Array.from(commit)};
}

export function sameHead(a, b) {
  const left = head(a), right = head(b);
  return left.revision === right.revision && equalBytes(left.commitId, right.commitId);
}

export function encodeBinding(namespace, recordId, parent, saved) {
  const data = new Uint8Array(88), view = new DataView(data.buffer);
  const before = head(parent), after = head(saved);
  data.set(DOMAIN); data.set(bytes16(namespace), 8); data.set(bytes16(recordId), 24);
  view.setBigUint64(40, BigInt(before.revision), true); data.set(before.commitId, 48);
  view.setBigUint64(64, BigInt(after.revision), true); data.set(after.commitId, 72);
  return data;
}

export function decodeBinding(value) {
  if (!(value instanceof Uint8Array) || value.length !== 88 || !equalBytes(value.subarray(0, 8), DOMAIN) ||
      !value.subarray(8, 24).some(Boolean) || !value.subarray(24, 40).some(Boolean)) {
    throw new WorkspaceVaultError('invalid_binding');
  }
  const view = new DataView(value.buffer, value.byteOffset, value.byteLength);
  return {
    namespace: bytes16(value.subarray(8, 24)), recordId: bytes16(value.subarray(24, 40)),
    parent: {revision: view.getBigUint64(40, true).toString(), commitId: Array.from(value.subarray(48, 64))},
    saved: {revision: view.getBigUint64(64, true).toString(), commitId: Array.from(value.subarray(72, 88))},
  };
}
