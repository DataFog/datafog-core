import { client, serve, HostError } from "./rpc.js";
import { readRecords, saveRecord, revoke, equalHead, equalBytes } from "./storage.js";
import { createVault, openVault, encryptCandidate, decryptCandidate, randomId } from "./vault.js";

let core;
let session;
let pending;
let generation = 0;
let controller;
let activeDone;
let finishActive;
const faults = { abortSave: false, loseAck: false, pause: null };
const barriers = new Map();
function guard(value) { if (value !== generation) throw new HostError("cancelled"); }
function active() { if (!session) throw new HostError("locked"); return session; }
function stopCore() { core?.stop(); core = undefined; }
async function run(task) {
  if (activeDone) throw new HostError("workspace_conflict");
  activeDone = new Promise(resolve => { finishActive = resolve; });
  try { return await task(); }
  finally { const finish = finishActive; activeDone = undefined; finishActive = undefined; finish(); }
}
function makeCore() {
  if (!core) core = client(new Worker(new URL("./core-worker.js", import.meta.url), { type: "module" }));
  return core;
}
function sameRecord(a, b) {
  return Boolean(a && b && a.version === b.version && equalHead(a.head, b.head)
    && ["vaultId", "workspaceId", "keyId", "keyNonce", "wrappedKey", "nonce", "ciphertext"].every(k => equalBytes(a[k], b[k])));
}
async function current(value, g) {
  const records = await readRecords(); guard(g);
  if (!equalBytes(records.vault?.accessGeneration, value.vault.accessGeneration) || !sameRecord(records.workspace, value.record)) throw new HostError("workspace_conflict");
  return records;
}
async function loaded(value, g) {
  if (core) return core;
  const worker = makeCore();
  const bundle = await decryptCandidate(value.dataKey, value.record);
  try {
    guard(g);
    const metadata = await worker.call("open", [bundle]); guard(g);
    if (!equalBytes(metadata.namespace, value.record.workspaceId) || !equalHead(metadata.head, value.record.head)) throw new HostError("invalid_envelope");
    return worker;
  } catch (error) { stopCore(); throw error; }
  finally { bundle.fill(0); }
}
async function checkpoint(name) {
  if (faults.pause !== name) return;
  await new Promise(resolve => { barriers.set(name, resolve); });
}
async function invalidate(locking) {
  const before = session;
  const done = activeDone;
  generation++;
  controller?.abort();
  pending = undefined;
  if (locking) session = undefined;
  stopCore();
  for (const resolve of barriers.values()) resolve();
  barriers.clear();
  await done;
  if (locking && before) await revoke(before.vault);
  return { status: locking ? "locked" : "cancelled" };
}

serve({
  status() { return { status: session ? "unlocked" : "locked" }; },
  create(password) {
    return run(async () => {
      if ((await readRecords()).vault) throw new HostError("already_exists");
      const g = ++generation; controller = new AbortController();
      stopCore(); const worker = makeCore();
      const namespace = randomId();
      const metadata = await worker.call("create", [namespace, randomId()]); guard(g);
      const bundle = await worker.call("exportBundle"); guard(g);
      const created = await createVault(password, namespace, bundle, metadata.head, controller.signal); guard(g);
      await saveRecord({ ...created, creating: true, authorized: () => generation === g }); guard(g);
      session = created;
      return { status: "unlocked" };
    });
  },
  unlock(password) {
    return run(async () => {
      const g = ++generation; session = undefined; pending = undefined; stopCore();
      controller?.abort(); controller = new AbortController();
      const {vault, workspace: record} = await readRecords(); guard(g);
      if (!vault) throw new HostError("not_found");
      const opened = await openVault(password, vault, record, controller.signal);
      try {
        guard(g);
        const worker = makeCore();
        const metadata = await worker.call("open", [opened.bundle]); guard(g);
        if (!equalBytes(metadata.namespace, record.workspaceId) || !equalHead(metadata.head, record.head)) throw new HostError("invalid_envelope");
        const value = { vault, record, dataKey: opened.dataKey };
        await current(value, g);
        session = value;
        return { status: "unlocked" };
      } catch (error) { stopCore(); throw error; }
      finally { opened.bundle.fill(0); }
    });
  },
  select(text, start, end, entityType) {
    return run(async () => {
      const value = active(), g = generation;
      await current(value, g);
      const worker = await loaded(value, g);
      if (pending) { const old = pending; pending = undefined; await worker.call("discard", [old.id]); guard(g); }
      const range = await worker.call("byteRange", [text, start, end]); guard(g);
      return { entityType, matchedText: text.slice(start, end), range };
    });
  },
  prepare(inputs) {
    return run(async () => {
      const value = active(), g = generation;
      await current(value, g);
      const worker = await loaded(value, g);
      if (pending) { const previous = pending; pending = undefined; await worker.call("discard", [previous.id]); guard(g); }
      const next = await worker.call("prepareBatch", [inputs, randomId()]); guard(g);
      pending = { ...next, generation: g };
      return { id: next.id, previews: next.previews };
    });
  },
  approve(id) {
    return run(async () => {
      const value = active(), g = generation;
      const change = pending;
      if (!change || change.id !== id || change.generation !== g) throw new HostError("stale_approval");
      await current(value, g);
      const worker = await loaded(value, g);
      if (!change.record) {
        const bundle = await worker.call("exportCandidate", [id]);
        try { guard(g); change.record = await encryptCandidate(value.dataKey, value.record, change.candidateHead, bundle); }
        finally { bundle.fill(0); }
      }
      await checkpoint("before-save"); guard(g);
      let committed = false;
      try {
        await saveRecord({ vault: value.vault, record: change.record, expectedHead: change.expectedHead,
          expectedGeneration: value.vault.accessGeneration, authorized: () => generation === g && session === value, abort: faults.abortSave });
        committed = true;
        value.record = change.record;
        await checkpoint("after-save"); guard(g);
        if (faults.loseAck) {
          const records = await readRecords(); guard(g);
          if (!sameRecord(records.workspace, change.record)) throw new HostError("workspace_conflict");
          throw new HostError("save_uncertain");
        }
        await worker.call("activate", [id, change.expectedHead, change.candidateHead]); guard(g);
        const texts = change.previews.map(preview => preview.text);
        pending = undefined;
        return { status: "saved", texts };
      } catch (error) {
        if (committed) { pending = undefined; stopCore(); }
        throw error;
      }
    });
  },
  restore(text) {
    return run(async () => {
      const value = active(), g = generation;
      await current(value, g);
      const worker = await loaded(value, g);
      const restored = await worker.call("restore", [text]); guard(g);
      return { status: "restored", text: restored };
    });
  },
  cancel() { return invalidate(false); },
  lock() { return invalidate(true); },
  // This entire extension is a non-shipping synthetic harness. Fault controls
  // synchronize real transactions; they neither expose secrets nor replace crypto.
  test(command, value) {
    if (command === "configure") { Object.assign(faults, value); return true; }
    if (command === "state") return { busy: Boolean(activeDone), paused: [...barriers.keys()], head: session?.record.head, pending: Boolean(pending), locked: !session };
    if (command === "release") { barriers.get(value)?.(); barriers.delete(value); return true; }
    throw new HostError("internal_error");
  },
});
