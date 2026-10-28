import { HostError } from "./rpc.js";

export const DATABASE = "datafog-synthetic-workspace-v1";
export function equalBytes(a, b) { return a instanceof Uint8Array && b instanceof Uint8Array && a.length === b.length && a.every((v, i) => v === b[i]); }
export function equalHead(a, b) {
  return Boolean(a && b && typeof a.revision === "string" && a.revision === b.revision
    && Array.isArray(a.commitId) && Array.isArray(b.commitId) && a.commitId.length === 16 && b.commitId.length === 16
    && Array.from({ length: 16 }, (_, i) => Number.isInteger(a.commitId[i]) && a.commitId[i] >= 0 && a.commitId[i] <= 255 && a.commitId[i] === b.commitId[i]).every(Boolean));
}
export function workspaceKey(id) { return `workspace:${Array.from(id, n => n.toString(16).padStart(2, "0")).join("")}`; }
function open() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE, 1);
    request.onupgradeneeded = () => request.result.createObjectStore("records");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(new HostError("storage_failure"));
    request.onblocked = () => reject(new HostError("storage_failure"));
  });
}
export async function readRecords() {
  const db = await open();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction("records", "readonly");
      let vault, workspace;
      const request = tx.objectStore("records").get("vault");
      request.onsuccess = () => {
        vault = request.result;
        if (vault?.workspaceId instanceof Uint8Array && vault.workspaceId.length === 16) {
          const next = tx.objectStore("records").get(workspaceKey(vault.workspaceId));
          next.onsuccess = () => { workspace = next.result; };
        }
      };
      tx.oncomplete = () => resolve({ vault, workspace });
      tx.onabort = () => reject(new HostError("storage_failure"));
      tx.onerror = () => {};
    });
  } finally { db.close(); }
}

/** A real read/compare/write transaction. The test-only abort forces IndexedDB rollback. */
export async function saveRecord({ vault, record, expectedHead, expectedGeneration, creating = false, authorized, abort = false }) {
  const db = await open();
  try {
    await new Promise((resolve, reject) => {
      const tx = db.transaction("records", "readwrite", { durability: "strict" });
      const store = tx.objectStore("records");
      let failure = new HostError("storage_failure");
      const stop = code => { failure = new HostError(code); tx.abort(); };
      const currentVault = store.get("vault");
      currentVault.onsuccess = () => {
        if (!authorized()) { stop("cancelled"); return; }
        if (creating ? currentVault.result !== undefined : !equalBytes(currentVault.result?.accessGeneration, expectedGeneration)) {
          stop(creating ? "already_exists" : "workspace_conflict"); return;
        }
        const existing = store.get(workspaceKey(record.workspaceId));
        existing.onsuccess = () => {
          if (!authorized()) { stop("cancelled"); return; }
          if (creating ? existing.result !== undefined : !equalHead(existing.result?.head, expectedHead)) { stop("workspace_conflict"); return; }
          if (creating) store.put(vault, "vault");
          store.put(record, workspaceKey(record.workspaceId));
          if (abort) stop("storage_failure");
        };
      };
      tx.oncomplete = () => resolve();
      tx.onabort = () => reject(failure);
      tx.onerror = () => {};
    });
  } finally { db.close(); }
}

export async function revoke(expectedVault) {
  const db = await open();
  try {
    await new Promise((resolve, reject) => {
      const tx = db.transaction("records", "readwrite", { durability: "strict" });
      const store = tx.objectStore("records");
      const request = store.get("vault");
      request.onsuccess = () => {
        const record = request.result;
        if (record && equalBytes(record.vaultId, expectedVault.vaultId)) {
          record.accessGeneration = crypto.getRandomValues(new Uint8Array(16));
          store.put(record, "vault");
        }
      };
      tx.oncomplete = () => resolve();
      tx.onabort = () => reject(new HostError("storage_failure"));
      tx.onerror = () => {};
    });
  } finally { db.close(); }
}
