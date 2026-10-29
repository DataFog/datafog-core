import { init, Workspace, workspaceByteRangeFromUtf16 } from "./vendor/package/index.js";
import { serve, HostError } from "./rpc.js";

const ready = init();
let workspace;
const plans = new Map();
let next = 1;
function active() { if (!workspace) throw new HostError("closed_workspace"); return workspace; }
function plan(id) { const value = plans.get(id); if (!value) throw new HostError("inactive_plan"); return value; }
serve({
  async create(namespace, operation) { await ready; workspace?.close(); plans.clear(); workspace = Workspace.create(namespace, operation); return metadata(); },
  async open(bytes) { await ready; workspace?.close(); plans.clear(); try { workspace = Workspace.openBundle(bytes); return metadata(); } finally { bytes.fill(0); } },
  exportBundle() { return active().exportBundle(); },
  prepareRecovery(operation) {
    const value = active().prepareRecovery(operation);
    const id = next++; plans.set(id, value);
    return { id, format:value.snapshotFormat(), ...value.heads() };
  },
  head() { return active().head(); },
  byteRange(text, start, end) { return workspaceByteRangeFromUtf16(text, start, end); },
  policy() { return active().policy(); },
  recall(text) { return active().recall(text); },
  preparePolicy(edits, operation) {
    const value=active().preparePolicyChanges(edits,operation);
    const id=next++;plans.set(id,value);
    return {id,previews:[],policyPreview:value.policyPreview(),format:value.snapshotFormat(),...value.heads()};
  },
  prepareBatch(inputs, operation) {
    const value = active().prepareBatch(inputs, operation);
    const id = next++; plans.set(id, value);
    return { id, previews: value.previews(), format:value.snapshotFormat(), ...value.heads() };
  },
  exportCandidate(id) { return plan(id).exportCandidateBundle(); },
  activate(id, parent, saved) { active().activateSaved(plan(id), parent, saved); plans.delete(id); },
  discard(id) { plan(id).discard(); plans.delete(id); },
  restore(text) { return active().restore(text); },
  close() { plans.clear(); workspace?.close(); workspace = undefined; },
});
function metadata() { return { namespace: active().namespace(), head: active().head() }; }
