// Synthetic host seam around the installed package and the existing encrypted MV3 host.
import {init, Workspace} from "@datafog/wasm";
import {createVault, openVault, encryptCandidate, createWorkspaceRecord, randomId} from "../examples/workspace-extension/src/vault.js";
import {readRecords, saveRecord} from "../examples/workspace-extension/src/storage.js";
import {exportBackup, openBackup} from "../examples/workspace-extension/src/backup.js";
import {HostError} from "../examples/workspace-extension/src/rpc.js";
await init();
let workspace, host, pending;
const sink = text => {document.querySelector("#workflow-sink").textContent = text;};
window.browserWorkspaceQualification = {
  async create(password) {
    workspace = Workspace.create(randomId(),randomId());
    const created = await createVault(password,workspace.namespace(),workspace.exportBundle(),workspace.head());
    await saveRecord({...created,creating:true,authorized:()=>true});
    host = {vault:created.vault,record:created.record,dataKey:created.dataKey};
    return {head:workspace.head(),recoveryCode:created.recoveryCode};
  },
  async open(credential,method="password") {
    const {vault,workspace:record} = await readRecords();
    const opened = await openVault(credential,vault,record,undefined,method);
    try {workspace = Workspace.openBundle(opened.bundle);}
    finally {opened.bundle.fill(0);}
    host = {vault,record,dataKey:opened.dataKey};
    return workspace.head();
  },
  stage(inputs) {
    if (pending) throw new HostError("pending_review");
    const change = workspace.prepareReview(inputs,randomId());
    pending = {change,previews:change.previews()};sink("");
    return {heads:change.heads(),previews:pending.previews};
  },
  async approve(approved,abort=false) {
    if (!pending) throw new HostError("stale_plan");
    if (approved !== true) {pending.change.discard();pending=undefined;throw new HostError("cancelled");}
    const {change,previews} = pending,heads=change.heads();
    const bundle=change.exportCandidateBundle();let record;
    try {record=await encryptCandidate(host.dataKey,host.record,heads.candidateHead,bundle);}
    finally {bundle.fill(0);}
    await saveRecord({vault:host.vault,record,expectedHead:heads.expectedHead,
      expectedGeneration:host.vault.accessGeneration,authorized:()=>true,abort});
    workspace.activateSaved(change,heads.expectedHead,heads.candidateHead);
    host.record=record;pending=undefined;sink(JSON.stringify(previews));
    return {previews,head:workspace.head()};
  },
  query:text=>workspace.recall(text),
  restore:text=>workspace.restore(text),
  report:text=>workspace.restoreReport(text),
  head:()=>workspace.head(),
  async fingerprint() {
    const bundle=workspace.exportBundle();
    try {return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bundle)));}
    finally {bundle.fill(0);}
  },
  async noStoredPlaintext(values) {
    const records=JSON.stringify(await readRecords());return values.every(value=>!records.includes(value));
  },
  async backup(password) {return Array.from(await exportBackup(host.vault,host.record,host.dataKey,password));},
  async recover(bytes,credential,method) {
    const opened=await openBackup(new Uint8Array(bytes),credential,method);
    let imported,change;
    try {
      imported=Workspace.openBundle(opened.bundle);opened.bundle.fill(0);
      const oldHead=imported.head();change=imported.prepareRecovery(randomId());
      const heads=change.heads();
      const created=await createWorkspaceRecord(opened.rootKey,opened.vault,change.exportCandidateBundle(),heads.candidateHead);
      await saveRecord({vault:opened.vault,record:created.record,creating:true,authorized:()=>true});
      imported.activateSaved(change,heads.expectedHead,heads.candidateHead);change=undefined;
      workspace=imported;imported=undefined;host={vault:opened.vault,record:created.record,dataKey:created.dataKey};
      return {oldHead,head:workspace.head(),profile:opened.vault.profile,
        freshKey:!created.record.keyId.every((byte,index)=>byte===opened.record.keyId[index]),
        freshNonce:!created.record.nonce.every((byte,index)=>byte===opened.record.nonce[index])};
    } finally {opened.bundle.fill(0);change?.discard();imported?.close();}
  },
  close() {pending?.change.discard();pending=undefined;workspace?.close();workspace=undefined;host=undefined;},
};
