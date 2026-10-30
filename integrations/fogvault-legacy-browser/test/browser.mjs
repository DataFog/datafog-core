import {init,Workspace} from './node_modules/@datafog/wasm/index.js';
import {FogVault,randomId} from './node_modules/@datafog/fogvault-browser/dist/client.mjs';
import {FogVaultWorkspace} from './node_modules/@datafog/core-fogvault-browser/index.mjs';
import {LegacyBackupImport} from './node_modules/@datafog/core-fogvault-legacy-browser/index.mjs';
import {createVault,openVault,toBase64url,encryptCandidate} from './reference/vault.js';
import {exportBackup} from './reference/backup.js';
const cases=[],names=[],vaults=new Set(),sessions=new Set();
const password='synthetic-only legacy password',text='🦀 alice@example.test';
const require=(v,m)=>{if(!v)throw Error(m);};
const equal=(a,b)=>a.length===b.length&&a.every((v,i)=>v===b[i]);
async function rejected(fn,code){try{await fn();}catch(e){require(!code||e.code===code,`expected ${code}, got ${e.code??e.message}`);return e;}throw Error('expected rejection');}
async function record(id,fn){try{const proof=await fn();cases.push({id,status:'pass',executed:true,...proof});}catch(e){cases.push({id,status:'fail',executed:true,code:e.code??e.name,message:e.message});throw e;}}
const options={limits:{payload:16*1024*1024,metadata:88,records:64},storageProfile:'arraybuffer',kdfProfiles:[{memory_kib:65536,passes:3}]};
async function destination(){const vault=new FogVault(options);vaults.add(vault);const name='datafog-synthetic-legacy-'+crypto.randomUUID();names.push(name);await vault.create(name,password);return{vault,name,recordId:randomId()};}
async function fixture(){
 const workspace=Workspace.create(randomId(),randomId());
 const value='alice@example.test',start=new TextEncoder().encode('🦀 ').length;
 const change=workspace.prepareReview([{text,scannerFindings:[{entityType:'EMAIL',matchedText:value,range:{unit:'utf8_bytes',start,end:start+value.length}}],userSelections:[]}],randomId());
 const marker=change.previews()[0].text,heads=change.heads();workspace.activateSaved(change,heads.expectedHead,heads.candidateHead);
 const bundle=workspace.exportBundle(),namespace=workspace.namespace(),head=workspace.head();
 const created=await createVault(password,namespace,bundle,head);
 const v2=await exportBackup(created.vault,created.record,created.dataKey,password);
 const body=JSON.parse(new TextDecoder().decode(v2)).payload;
 body.version=1;for(const key of ['profile','salt','rootNonce','wrappedRoot'])body.vault[key]=key==='profile'?created.vault[key]:toBase64url(created.vault[key]);
 const nonce=crypto.getRandomValues(new Uint8Array(12));
 const tag=new Uint8Array(await crypto.subtle.encrypt({name:'AES-GCM',iv:nonce,tagLength:128,additionalData:new TextEncoder().encode(`DFGB1:AUTH:${JSON.stringify(body)}`)},created.dataKey,new Uint8Array()));
 const v1=new TextEncoder().encode(JSON.stringify({payload:body,nonce:toBase64url(nonce),tag:toBase64url(tag)}));
 const invalidHead={...head,revision:String(BigInt(head.revision)+1n)};
 const badRecord=await encryptCandidate(created.dataKey,created.record,invalidHead,workspace.exportBundle());
 const badHead=await exportBackup(created.vault,badRecord,created.dataKey,password);
 workspace.close();return{v1,v2,badHead,marker,namespace,head,recovery:created.recoveryCode};
}
async function open(bytes,extra={}){const s=await LegacyBackupImport.open({Workspace,archive:bytes,credential:password,...extra});sessions.add(s);return s;}
const save=(session,f,approved=true)=>session.commit({vault:f.vault,recordId:f.recordId,operationId:randomId(),requestId:randomId(),approved});
window.coreFogVaultDone=(async()=>{let failure;try{
 await init();
 await record('legacy_fixed_kdf_profiles_match_reference_vectors',async()=>{
  for(const name of ['argon2id.json','argon2id-backup.json']){
   const vector=await(await fetch('./'+name)).json(),worker=new Worker('./node_modules/@datafog/core-fogvault-legacy-browser/kdf-worker.mjs',{type:'module'});
   try{const key=await new Promise((resolve,reject)=>{worker.onmessage=({data})=>data.key?resolve(data.key):reject(Error(data.error));worker.onerror=reject;worker.postMessage({password:vector.password,salt:Uint8Array.from(vector.saltHex.match(/../g),s=>parseInt(s,16)),profile:vector.profile??1});});
    require(Array.from(key,b=>b.toString(16).padStart(2,'0')).join('')===vector.outputHex,'fixed Argon profile differs');key.fill(0);
   }finally{worker.terminate();}
  }
 });
 const source=await fixture();
 await record('legacy_v1_v2_password_recovery_actual_kdf_reopen',async()=>{
  const combinations=[];for(const archive of [source.v1,source.v2])for(const method of ['password','recovery']){
   const before=Uint8Array.from(archive),s=await open(archive,{method,credential:method==='password'?password:source.recovery});
   const preview=s.preview();require(equal(preview.namespace,source.namespace),'namespace changed');
   const f=await destination(),owner=await save(s,f);require(owner.restore(source.marker)===text,'restore differs');
   require(BigInt(owner.head().revision)>BigInt(source.head.revision),'recovery head not fresh');
   const head=owner.head();await owner.close();
   const reopened=new FogVault(options);vaults.add(reopened);await reopened.open(f.name);await reopened.unlock(password);
   const recovered=await FogVaultWorkspace.open({Workspace,vault:reopened,recordId:f.recordId,expectedNamespace:source.namespace,expectedHead:head});
   require(recovered.restore(source.marker)===text,'reopen lost alias');await recovered.close();
   require(equal(before,archive),'source archive mutated');await rejected(()=>s.preview(),'import_closed');
   combinations.push({id:`v${archive===source.v1?1:2}_${method}`,status:'pass',executed:true});
  }return{combinations};
 });
 await record('legacy_unused_slot_tampering_and_profile_versions_reject',async()=>{
  for(const raw of [source.v1,source.v2]){
   const mutate=fn=>{const o=JSON.parse(new TextDecoder().decode(raw));fn(o);return new TextEncoder().encode(JSON.stringify(o));};
   await rejected(()=>open(mutate(o=>{const s=o.payload.vault.recovery.wrappedRoot;o.payload.vault.recovery.wrappedRoot=(s[0]==='A'?'B':'A')+s.slice(1);})), 'invalid_backup');
   await rejected(()=>open(mutate(o=>{const s=o.payload.vault.wrappedRoot;o.payload.vault.wrappedRoot=(s[0]==='A'?'B':'A')+s.slice(1);}),{method:'recovery',credential:source.recovery}),'invalid_backup');
   await rejected(()=>open(mutate(o=>o.payload.vault.profile=99)),'invalid_backup');
   await rejected(()=>open(mutate(o=>o.payload.version=3)),'invalid_backup');
   await rejected(()=>open(mutate(o=>o.payload.extra='unsupported')),'invalid_backup');
  }
 });
 await record('legacy_wrong_credentials_namespace_and_authenticated_head_reject',async()=>{
  await rejected(()=>open(source.v1,{credential:'synthetic wrong password'}),'invalid_credentials');
  await rejected(()=>open(source.v2,{method:'recovery',credential:'DFR1.'+'A'.repeat(43)}),'invalid_credentials');
  await rejected(()=>open(source.v1,{expectedNamespace:randomId()}),'binding_mismatch');
  await rejected(()=>open(source.v1,{expectedHead:{...source.head,revision:'999'}}),'binding_mismatch');
  await rejected(()=>open(source.badHead),'binding_mismatch');
 });
 await record('legacy_literal_approval_opaque_session_and_occupied_destination',async()=>{
  await rejected(()=>new LegacyBackupImport(),'invalid_import');
  const s=await open(source.v1),f=await destination();
  require(!('bundle'in s)&&!('rootKey'in s)&&!('dataKey'in s),'raw source material escaped');
  await rejected(()=>save(s,f,1),'approval_required');require(!f.vault.locked,'rejected approval changed destination');
  const existing=await FogVaultWorkspace.create({Workspace,vault:f.vault,recordId:f.recordId,namespace:randomId()});const before=await f.vault.receipt(),baseline=existing.counts();
  await rejected(()=>save(s,f),'occupied_destination');
  await f.vault.unlock(password);const after=await f.vault.receipt();require(before.generation===after.generation,'occupied import wrote state');
  const reopened=await FogVaultWorkspace.open({Workspace,vault:f.vault,recordId:f.recordId});require(reopened.counts().operations===baseline.operations,'occupied data changed');await reopened.close();
 });
 await record('legacy_worker_cancel_and_closed_session_never_publish',async()=>{
  const controller=new AbortController(),attempt=open(source.v2,{signal:controller.signal});setTimeout(()=>controller.abort(),5);await rejected(()=>attempt,'cancelled');
  const late=new AbortController(),s=await open(source.v1,{signal:late.signal});late.abort();await rejected(()=>s.preview(),'import_closed');
  const f=await destination(),before=await f.vault.receipt();await rejected(()=>save(s,f),'import_closed');require((await f.vault.receipt()).generation===before.generation,'closed import published');
  const s2=await open(source.v1);await s2.close();await s2.close();await rejected(()=>s2.preview(),'import_closed');
  const afterStart=new AbortController(),s3=await open(source.v1,{signal:afterStart.signal}),pendingDestination=await destination();
  const request=randomId(),publication=s3.commit({vault:pendingDestination.vault,recordId:pendingDestination.recordId,operationId:randomId(),requestId:request,approved:true});afterStart.abort();
  const error=await rejected(()=>publication,'import_requires_reopen');require(equal(error.requestId,request)&&error.requiresReopen===true,'uncertain request identity missing');require(pendingDestination.vault.locked,'late cancellation did not lock destination');
 });
 await record('legacy_confirmed_save_cancel_cleanup_failure_preserves_request',async()=>{
  const controller=new AbortController(),session=await open(source.v1,{signal:controller.signal}),f=await destination();
  const request=randomId();let published=false,lockRejected=false;
  const transport=new Proxy(f.vault,{get(target,key){
   if(key==='save')return async(...args)=>{const result=await target.save(...args);published=true;controller.abort();return result;};
   if(key==='lock')return async()=>{await target.lock();if(!lockRejected){lockRejected=true;throw Error('synthetic cleanup rejection after actual lock');}};
   const value=Reflect.get(target,key,target);return typeof value==='function'?value.bind(target):value;
  }});
  const error=await rejected(()=>session.commit({vault:transport,recordId:f.recordId,operationId:randomId(),requestId:request,approved:true}),'import_requires_reopen');
  require(published&&lockRejected&&f.vault.locked,'real publication/lock cleanup path not exercised');
  require(equal(error.requestId,request)&&error.requiresReopen===true&&error.cleanupFailed===true,'outcome identity lost to cleanup failure');
  require(!('text'in error)&&!('owner'in error),'failed operation released handoff');
  await rejected(()=>session.preview(),'import_closed');await rejected(()=>session.close(),'cleanup_failed');
  const fresh=new FogVault(options);vaults.add(fresh);await fresh.open(f.name);await fresh.unlock(password);
  const reconciled=await fresh.reconcileLatest(request);require(equal(reconciled.receipt.request_id,request),'fresh reconciliation lost exact save identity');
  const reopened=await FogVaultWorkspace.open({Workspace,vault:fresh,recordId:f.recordId,expectedNamespace:source.namespace});
  require(reopened.restore(source.marker)===text,'confirmed saved state not recoverable after fresh authentication');await reopened.close();
 });
 await record('legacy_archive_admission_and_source_preservation',async()=>{
  await rejected(()=>open(new Uint8Array(24*1024*1024+1)),'resource_limit');
  await rejected(()=>open(new Uint8Array(new SharedArrayBuffer(32))),'invalid_transport');
  let invoked=false;const disguised=new Uint8Array(new SharedArrayBuffer(32));Object.defineProperty(disguised,'buffer',{get(){invoked=true;return new ArrayBuffer(32);}});await rejected(()=>open(disguised),'invalid_transport');require(!invoked,'buffer accessor executed');
  const sourceCopy=Uint8Array.from(source.v1),controller=new AbortController();controller.abort();await rejected(()=>open(sourceCopy,{signal:controller.signal}),'cancelled');require(equal(sourceCopy,source.v1),'cancel changed source');
  const bad=Uint8Array.from(source.v1);bad[0]=0;const before=Uint8Array.from(bad);await rejected(()=>open(bad),'invalid_backup');require(equal(before,bad),'failure changed source');
 });
 }catch(e){failure={code:e.code??e.name,message:e.message};}
 finally{for(const s of sessions)await s.close().catch(()=>{});for(const v of vaults)await v.close().catch(()=>{});for(const name of names)await new Promise(resolve=>{const r=indexedDB.deleteDatabase(name);r.onsuccess=r.onerror=r.onblocked=resolve;});}
 return{pass:!failure,executed:true,userAgent:navigator.userAgent,cases,failure};})();
