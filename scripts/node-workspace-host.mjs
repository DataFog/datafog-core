// Synthetic reference host: storage/encryption are host responsibilities, not SDK exports.
import {DatabaseSync} from "node:sqlite";
import {createCipheriv,createDecipheriv,randomBytes,timingSafeEqual} from "node:crypto";
import {Workspace} from "@datafog/node";

export class HostFailure extends Error {
  constructor(code) {super(code);this.code=code;}
}
const failure=code=>new HostFailure(code);
function scopeValue(scope) {
  if(!scope || typeof scope.tenantId!=="string" || typeof scope.workspaceId!=="string" ||
      !/^[a-zA-Z0-9_-]{1,64}$/.test(scope.tenantId) || !/^[a-zA-Z0-9_-]{1,64}$/.test(scope.workspaceId)) throw failure("workspace_unavailable");
  return {tenantId:scope.tenantId,workspaceId:scope.workspaceId};
}
function headValue(head) {
  if(!head || typeof head.revision!=="string" || !/^(0|[1-9][0-9]{0,19})$/.test(head.revision) ||
      BigInt(head.revision)>18446744073709551615n || !Array.isArray(head.commitId) || head.commitId.length!==16 ||
      head.commitId.some(value=>!Number.isInteger(value)||value<0||value>255)) throw failure("workspace_conflict");
  return {revision:head.revision,commitId:[...head.commitId]};
}
const commitHex=head=>Buffer.from(head.commitId).toString("hex");
const sameHead=(left,right)=>left.revision===right.revision && commitHex(left)===commitHex(right);
function aad(scope,head,namespace) {
  return Buffer.from(JSON.stringify(["synthetic-node-workspace-host-v1",scope.tenantId,scope.workspaceId,namespace,head.revision,commitHex(head)]));
}
function storedHead(row) {return {revision:row.revision,commitId:[...Buffer.from(row.commit_id,"hex")]};}
function databaseError(error) {
  if(error instanceof HostFailure) return error;
  return failure(error?.code==="ERR_SQLITE_ERROR" && [5,6].includes(error.errcode) ? "storage_busy" : "storage_failure");
}
export class EncryptedWorkspaceStore {
  #database;#resolveKey;
  constructor(file,resolveKey) {
    this.#resolveKey=resolveKey;
    try {
      this.#database=new DatabaseSync(file,{timeout:250,defensive:true});
      this.#database.exec(`PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS workspaces(
          tenant TEXT NOT NULL, workspace TEXT NOT NULL, revision TEXT NOT NULL,
          commit_id TEXT NOT NULL, namespace TEXT NOT NULL, nonce BLOB NOT NULL,
          ciphertext BLOB NOT NULL, tag BLOB NOT NULL, PRIMARY KEY(tenant,workspace)
        ) STRICT`);
    } catch(error) {this.#database?.close();throw databaseError(error);}
  }
  #key(scope) {
    // The trusted host authorizes the full tenant/workspace before any decrypt.
    let value;
    try {value=this.#resolveKey(scope);} catch {throw failure("workspace_unavailable");}
    if(!(value instanceof Uint8Array) || value.length!==32) throw failure("workspace_unavailable");
    return Buffer.from(value);
  }
  #row(scope) {
    return this.#database.prepare("SELECT * FROM workspaces WHERE tenant=? AND workspace=?").get(scope.tenantId,scope.workspaceId);
  }
  #transaction(action) {
    try {
      this.#database.exec("BEGIN IMMEDIATE");
      const result=action();
      this.#database.exec("COMMIT");
      return result;
    } catch(error) {
      let rejected=databaseError(error);
      if(this.#database.isTransaction) {
        try {this.#database.exec("ROLLBACK");} catch {rejected=failure("storage_failure");}
      }
      throw rejected;
    }
  }
  #envelope(scope,key,bundle) {
    let workspace;
    try {
      workspace=Workspace.openBundle(bundle);
      const head=workspace.head(),namespace=Buffer.from(workspace.namespace()).toString("hex");
      const nonce=randomBytes(12),cipher=createCipheriv("aes-256-gcm",key,nonce);
      cipher.setAAD(aad(scope,head,namespace));
      return {head,namespace,nonce,ciphertext:Buffer.concat([cipher.update(bundle),cipher.final()]),tag:cipher.getAuthTag()};
    } catch {throw failure("invalid_snapshot");}
    finally {workspace?.close();}
  }
  initialize(rawScope,bundle) {
    const scope=scopeValue(rawScope),key=this.#key(scope);
    try {
      const envelope=this.#envelope(scope,key,bundle);
      return this.#transaction(()=>{
        if(this.#row(scope))throw failure("workspace_conflict");
        this.#database.prepare("INSERT INTO workspaces VALUES (?,?,?,?,?,?,?,?)").run(scope.tenantId,scope.workspaceId,envelope.head.revision,commitHex(envelope.head),envelope.namespace,envelope.nonce,envelope.ciphertext,envelope.tag);
        return envelope.head;
      });
    } finally {key.fill(0);}
  }
  load(rawScope) {
    const scope=scopeValue(rawScope),key=this.#key(scope);
    let bundle,workspace;
    try {
      const row=this.#row(scope);
      if(!row)throw failure("workspace_unavailable");
      const head=headValue(storedHead(row));
      const decipher=createDecipheriv("aes-256-gcm",key,row.nonce);
      decipher.setAAD(aad(scope,head,row.namespace));decipher.setAuthTag(row.tag);
      const pieces=[];let plaintext;
      try {
        pieces.push(decipher.update(row.ciphertext),decipher.final());
        plaintext=Buffer.concat(pieces);bundle=new Uint8Array(plaintext);
      } finally {for(const piece of pieces)piece.fill(0);plaintext?.fill(0);}
      workspace=Workspace.openBundle(bundle);
      if(!sameHead(workspace.head(),head) || Buffer.from(workspace.namespace()).toString("hex")!==row.namespace)throw failure("invalid_snapshot");
      return {bundle,head};
    } catch(error) {
      bundle?.fill(0);
      throw error instanceof HostFailure ? error : failure("invalid_snapshot");
    } finally {workspace?.close();key.fill(0);}
  }
  saveIfHeadMatches(rawScope,expectedHead,candidateHead,bundle,fault) {
    const scope=scopeValue(rawScope),key=this.#key(scope);
    try {
      const expected=headValue(expectedHead),candidate=headValue(candidateHead);
      const envelope=this.#envelope(scope,key,bundle);
      if(!sameHead(envelope.head,candidate))throw failure("workspace_conflict");
      this.#transaction(()=>{
        const row=this.#row(scope);
        if(!row || !sameHead(storedHead(row),expected) || row.namespace!==envelope.namespace)throw failure("workspace_conflict");
        const changed=this.#database.prepare("UPDATE workspaces SET revision=?,commit_id=?,nonce=?,ciphertext=?,tag=? WHERE tenant=? AND workspace=? AND revision=? AND commit_id=?")
          .run(candidate.revision,commitHex(candidate),envelope.nonce,envelope.ciphertext,envelope.tag,scope.tenantId,scope.workspaceId,expected.revision,commitHex(expected));
        if(changed.changes!==1)throw failure("workspace_conflict");
        fault?.("before_commit");
      });
      fault?.("after_commit");
      const row=this.#row(scope);
      if(!row || !sameHead(storedHead(row),candidate))throw failure("workspace_conflict");
      return candidate;
    } finally {key.fill(0);}
  }
  close() {this.#database.close();}
}

/** Test host boundary: approval, persistence, activation and release remain explicit. */
export async function runPreparedRequest({store,scope,requestId,input,operationId,approve,release,signal,fault}) {
  if(typeof requestId!=="string" || !/^[a-zA-Z0-9_-]{1,64}$/.test(requestId))throw failure("invalid_request");
  const owner=scopeValue(scope);
  let workspace,plan,bytes;
  const cancelled=()=>{if(signal?.aborted)throw failure("cancelled");};
  try {
    cancelled();
    const saved=store.load(owner);
    try {workspace=Workspace.openBundle(saved.bundle);} finally {saved.bundle.fill(0);}
    plan=workspace.prepareReview([input],operationId);
    const previews=plan.previews(),heads=plan.heads();
    const presentation=structuredClone({requestId,head:heads.expectedHead,previews,snapshotFormat:plan.snapshotFormat()});
    const approvedPresentation=JSON.stringify(presentation);
    if(!await approve(presentation))throw failure("cancelled");
    if(JSON.stringify(presentation)!==approvedPresentation)throw failure("invalid_approval");
    cancelled();bytes=plan.exportCandidateBundle();
    const confirmed=store.saveIfHeadMatches(owner,heads.expectedHead,heads.candidateHead,bytes,fault);
    cancelled();workspace.activateSaved(plan,heads.expectedHead,confirmed);
    cancelled();
    await release({requestId,text:previews[0].text,head:confirmed});
    return {head:confirmed};
  } finally {bytes?.fill(0);workspace?.close();}
}

/** Trusted request context supplies scope; provider output never chooses it. */
export function restoreStoredResponse({store,scope,requestId,text}) {
  if(typeof requestId!=="string" || !/^[a-zA-Z0-9_-]{1,64}$/.test(requestId))throw failure("invalid_request");
  const saved=store.load(scopeValue(scope));let workspace;
  try {
    workspace=Workspace.openBundle(saved.bundle);
    return {requestId,text:workspace.restore(text)};
  } finally {saved.bundle.fill(0);workspace?.close();}
}
