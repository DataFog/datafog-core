import {FogVaultWorkspace} from '@datafog/core-fogvault-browser';
import {openBackup,MAX_BACKUP_BYTES} from './backup-read.mjs';
import {LegacyImportError} from './error.mjs';
export {LegacyImportError};
const construct=Symbol('authenticated legacy session'),states=new WeakMap();
const fail=code=>{throw new LegacyImportError(code);};
const same=(a,b)=>a.length===b.length&&a.every((v,i)=>v===b[i]);
const typedPrototype=Object.getPrototypeOf(Uint8Array.prototype);
const bufferOf=Object.getOwnPropertyDescriptor(typedPrototype,'buffer').get;
const lengthOf=Object.getOwnPropertyDescriptor(typedPrototype,'length').get;
function bytes(value,length,maximum=MAX_BACKUP_BYTES){
 if(!ArrayBuffer.isView(value)||Object.getPrototypeOf(value)!==Uint8Array.prototype)fail('invalid_transport');
 for(const key of ['buffer','byteLength','byteOffset','length','constructor',Symbol.iterator])if(Object.hasOwn(value,key))fail('invalid_transport');
 const buffer=bufferOf.call(value),size=lengthOf.call(value);
 if(!(buffer instanceof ArrayBuffer)||buffer.detached)fail('invalid_transport');
 if(size>maximum)fail('resource_limit');
 if(length!==undefined&&size!==length)fail('invalid_identity');
 const result=new Uint8Array(size);Uint8Array.prototype.set.call(result,value);return result;
}
function head(value){
 if(!value||typeof value.revision!=='string'||!/^(0|[1-9][0-9]{0,19})$/.test(value.revision)||BigInt(value.revision)>18446744073709551615n)fail('invalid_head');
 if(!(value.commitId instanceof Uint8Array)&&(!Array.isArray(value.commitId)||value.commitId.length!==16||value.commitId.some(v=>!Number.isInteger(v)||v<0||v>255)))fail('invalid_head');
 const commitId=value.commitId instanceof Uint8Array?bytes(value.commitId,16):Uint8Array.from(value.commitId);
 return {revision:value.revision,commitId:Array.from(commitId)};
}
function state(session){const s=states.get(session);if(!s||s.closed)fail('import_closed');return s;}
async function closeState(s){
 s.closed=true;s.bundle?.fill(0);s.bundle=null;s.controller.abort();s.signal?.removeEventListener('abort',s.abort);
 try{await s.vault?.lock();return true;}catch{s.cleanupFailed=true;return false;}
}
/** Import-only supported reference archives; never a generic archive decryptor. */
export class LegacyBackupImport {
 constructor(token,s){if(token!==construct)fail('invalid_import');states.set(this,s);Object.freeze(this);}
 static async open({Workspace,archive,credential,method='password',signal,expectedNamespace,expectedHead}){
  if(!['password','recovery'].includes(method))fail('invalid_credentials');
  if(typeof credential!=='string'||!credential.isWellFormed()||credential.length>1024)fail('invalid_credentials');
  const wantedNamespace=expectedNamespace===undefined?undefined:bytes(expectedNamespace,16),wantedHead=expectedHead===undefined?undefined:head(expectedHead);
  if(signal?.aborted)fail('cancelled');
  const source=bytes(archive),controller=new AbortController();
  const s={Workspace,controller,signal,closed:false,busy:false,bundle:null,vault:null};
  s.abort=()=>{void closeState(s);};
  signal?.addEventListener('abort',s.abort,{once:true});
  let opened,workspace;
  try{
   if(signal?.aborted){s.abort();fail('cancelled');}
   opened=await openBackup(source,credential,method,controller.signal);
   if(s.closed||controller.signal.aborted)fail('cancelled');
   workspace=Workspace.openBundle(opened.bundle);
   const namespace=bytes(workspace.namespace(),16),saved=head(workspace.head());
   if(!same(namespace,opened.vault.workspaceId)||saved.revision!==opened.record.head.revision||!same(saved.commitId,opened.record.head.commitId)
      ||wantedNamespace&&!same(namespace,wantedNamespace)||wantedHead&&(saved.revision!==wantedHead.revision||!same(saved.commitId,wantedHead.commitId)))fail('binding_mismatch');
   s.namespace=namespace;s.head=saved;s.exportedAt=opened.exportedAt;s.counts=workspace.counts();s.bundle=opened.bundle;
   return new LegacyBackupImport(construct,s);
  }catch(error){opened?.bundle.fill(0);await closeState(s);throw error instanceof LegacyImportError?error:new LegacyImportError(error.code??'invalid_backup');}
  finally{workspace?.close();source.fill(0);}
 }
 preview(){const s=state(this);return {exportedAt:s.exportedAt,namespace:Uint8Array.from(s.namespace),head:structuredClone(s.head),counts:structuredClone(s.counts)};}
 async commit({vault,recordId,operationId,requestId,approved}){
  const s=state(this);if(approved!==true)fail('approval_required');if(s.busy)fail('operation_in_progress');
  const record=bytes(recordId,16),operation=bytes(operationId,16),request=bytes(requestId,16);
  s.busy=true;s.vault=vault;let owner;
  try{
   owner=await FogVaultWorkspace.importAuthenticatedBundle({Workspace:s.Workspace,vault,recordId:record,bundle:s.bundle,
    expectedNamespace:s.namespace,expectedHead:s.head,operationId:operation,requestId:request,approved:true});
   if(s.closed||s.controller.signal.aborted){await owner.close();fail('cancelled');}
   s.vault=null;await closeState(s);return owner;
  }catch(error){
   const uncertain=s.closed||s.controller.signal.aborted||['OutcomeUnknown','outcome_unknown','invalid_save_confirmation'].includes(error?.code);
   const cleanupSucceeded=await closeState(s);
   if(uncertain||!cleanupSucceeded||s.cleanupFailed){const failure=new LegacyImportError('import_requires_reopen');failure.requestId=request.slice();failure.requiresReopen=true;if(s.cleanupFailed)failure.cleanupFailed=true;throw failure;}
   throw error;
  }
 }
 async close(){const s=states.get(this);if(s&&!s.closed)await closeState(s);if(s?.cleanupFailed)fail('cleanup_failed');}
}
