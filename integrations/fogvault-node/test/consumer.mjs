import test from 'node:test';
import assert from 'node:assert/strict';
import {randomBytes} from 'node:crypto';
import {mkdtempSync,mkdirSync,rmSync,existsSync,readFileSync} from 'node:fs';
import path from 'node:path';
import {Worker} from 'node:worker_threads';
const {NativeWorkspace,PreparedChange,DataFogVaultError}=await import(process.env.DATAFOG_FOGVAULT_NODE_IMPORT??new URL('../index.mjs',import.meta.url).href);
const base=process.env.DATAFOG_NODE_TEST_TMP??path.resolve('target/fogvault-node-tests');mkdirSync(base,{recursive:true});
const id=()=>randomBytes(16), t='synthetic-tenant';
function fixture(options={}){const temp=mkdtempSync(path.join(base,'owner-'));const config={tenant:t,protectedControlPath:path.join(temp,'control'),vaultPath:path.join(temp,'vault.sqlite'),password:Buffer.from('synthetic-only node integration password'),recordId:id(),namespace:id(),operationId:id(),requestId:id(),profile:'password_only',...options};return {temp,config,owner:()=>NativeWorkspace.createNew(config),cleanup:()=>rmSync(temp,{recursive:true,force:true})};}
function open(config,expectedHead){const {operationId,requestId,...rest}=config;return NativeWorkspace.openExisting({...rest,...(expectedHead?{expectedHead}:{})});}
function review(text='alice@example.test'){const matched='alice@example.test';const found=text.includes(matched)?matched:text;const start=Buffer.byteLength(text.slice(0,text.indexOf(found)));return {text,scannerFindings:[{entityType:'EMAIL',matchedText:found,range:{unit:'utf8_bytes',start,end:start+Buffer.byteLength(found)},confidence:.95}],userSelections:[],corrections:[]};}
function saved(owner,text='alice@example.test'){const plan=owner.prepareReview(t,[review(text)],id());return owner.commit(t,plan,true,id());}
function code(expected){return e=>e instanceof DataFogVaultError&&(Array.isArray(expected)?expected.includes(e.code):e.code===expected);}

test('installed_node_roundtrip_approval_lock_reopen_unicode',()=>{
 const f=fixture();let owner;
 try{owner=f.owner();const text='🦀 alice@example.test';const p=owner.prepareReview(t,[review(text)],id());const local=p.previews(t)[0];assert.equal(local.transformations[0].sourceUtf16Range.start,3);assert.equal(typeof p.heads(t).candidate.revision,'string');assert.equal(p.snapshotFormat(t).currentVersion,3);assert.throws(()=>owner.commit(t,p,1,id()),code('approval_required'));assert.throws(()=>owner.commit(t,p,false,id()),code('approval_required'));const committed=owner.commit(t,p,true,id());assert.equal(committed.recordRevision,'2');assert.equal(typeof committed.receipt.generation,'string');assert.throws(()=>p.previews(t),code('inactive_plan'));assert.equal(owner.restore(t,committed.previews[0]),text);const pending=owner.prepareReview(t,[review('bob@example.test')],id());const stream=owner.bufferedRestore(t);owner.lock(t);assert.throws(()=>pending.previews(t),code(['inactive_plan','closed_workspace']));assert.throws(()=>stream.feed(t,Buffer.from('x')),code('inactive_stream'));owner.close(t);owner=open(f.config,committed.head);assert.equal(owner.restore(t,committed.previews[0]),text);owner.close(t);owner=null;}finally{owner?.close(t);f.cleanup();}
});

test('installed_node_tenant_foreign_and_hostile_transport_reject',async()=>{
 const a=fixture(),b=fixture();let x,y;
 try{x=a.owner();y=b.owner();const p=x.prepareReview(t,[review()],id());assert.throws(()=>x.head('another-tenant'),code('wrong_tenant'));assert.throws(()=>y.commit(t,p,true,id()),code('foreign_change'));assert.throws(()=>x.commit(t,{},true,id()),code('foreign_change'));assert.throws(()=>new PreparedChange(),code('foreign_change'));let called=false;const input=review();Object.defineProperty(input,'text',{get(){called=true;throw Error('getter must not run')},enumerable:true});assert.throws(()=>x.prepareReview(t,[input],id()),code('invalid_transport'));assert.equal(called,false);assert.throws(()=>x.prepareReview(t,[new Proxy(review(),{})],id()),code('invalid_transport'));assert.throws(()=>x.restore(t,'\ud800'),code('invalid_encoding'));assert.throws(()=>open(a.config,{revision:9007199254740992,commitId:id()}),code('invalid_transport'));assert.throws(()=>open(a.config,{revision:1,commitId:id()}),code('invalid_head'));const shared=new Uint8Array(new SharedArrayBuffer(16));assert.throws(()=>x.prepareReview(t,[review()],shared),code('invalid_transport'));assert.throws(()=>x.prepareReview(t,[review()],new Uint8Array(15)),code('invalid_transport'));const evil=new Uint8Array(16);Object.defineProperty(evil,'buffer',{get(){called=true;throw Error('getter')}});assert.throws(()=>x.prepareReview(t,[review()],evil),code('invalid_transport'));assert.equal(called,false);const c=structuredClone(x);assert.throws(()=>NativeWorkspace.prototype.head.call(c,t),code('invalid_owner'));
 const workerResult=await new Promise((resolve,reject)=>{const worker=new Worker(new URL('./worker.mjs',import.meta.url),{workerData:{module:process.env.DATAFOG_FOGVAULT_NODE_IMPORT??new URL('../index.mjs',import.meta.url).href,owner:x,tenant:t}});worker.once('message',resolve);worker.once('error',reject);});assert.equal(workerResult,'invalid_owner');
 }finally{x?.close(t);y?.close(t);a.cleanup();b.cleanup();}
 const fresh=fixture();try{assert.throws(()=>NativeWorkspace.createNew({...fresh.config,password:new Uint8Array(new SharedArrayBuffer(32))}),code('invalid_transport'));assert.equal(existsSync(fresh.config.protectedControlPath),false);assert.throws(()=>NativeWorkspace.createNew({...fresh.config,profile:'password_and_device'}),code('unsupported_profile'));assert.equal(existsSync(fresh.config.protectedControlPath),false);}finally{fresh.cleanup();}
});

test('installed_node_conflict_and_payload_admission_preserve_saved_state',()=>{
 const f=fixture({maxPayloadBytes:512});let first,second;
 try{first=f.owner();const initial=saved(first);second=open(f.config,initial.head);const stale=second.prepareReview(t,[review('bob@example.test')],id());const next=saved(first,'carol@example.test');assert.throws(()=>second.commit(t,stale,true,id()),code('vault_conflict'));assert.throws(()=>second.head(t),code('reopen_required'));second.close(t);second=null;const huge=`${'a'.repeat(2048)}@example.test`;const oversized=first.prepareReview(t,[review(huge)],id());assert.throws(()=>first.commit(t,oversized,true,id()),code('resource_limit'));assert.throws(()=>first.restore(t,initial.previews[0]),code('reopen_required'));first.close(t);first=open(f.config,next.head);assert.equal(first.restore(t,initial.previews[0]),'alice@example.test');assert.equal(first.restore(t,next.previews[0]),'carol@example.test');first.close(t);first=null;}finally{first?.close(t);second?.close(t);f.cleanup();}
});

test('installed_node_conversation_pending_and_scoped_restore',()=>{
 const f=fixture();let owner;
 try{owner=f.owner();const logical=id(),operation={id:id(),occurredAtMs:1};const conversation={id:logical,surface:'synthetic-local',siteId:null,siteIdByteBudget:64};const plan=owner.prepareConversationSend(t,[{input:review(),conversation}],operation);const intent=plan.sendIntents(t)[0];const result=owner.commit(t,plan,true,id());assert.throws(()=>owner.restoreInConversation(t,result.previews[0],logical),code('conversation_pending'));owner.close(t);owner=open(f.config,result.head);assert.throws(()=>owner.restoreReportInConversation(t,result.previews[0],logical),code('conversation_pending'));const observed=owner.prepareObservedSend(t,intent,logical,'synthetic-local','synthetic-chat',{id:id(),occurredAtMs:2});owner.commit(t,observed,true,id());assert.equal(owner.restoreInConversation(t,result.previews[0],logical),'alice@example.test');assert.equal(owner.restoreReportInConversation(t,result.previews[0],logical).restoredMarkers,1);assert.throws(()=>owner.restoreInConversation(t,result.previews[0],id()),code('conversation_unknown'));owner.close(t);owner=null;}finally{owner?.close(t);f.cleanup();}
});

test('installed_node_streams_terminal_transport_and_lifetimes',()=>{
 const f=fixture();let owner;
 try{owner=f.owner();const r=saved(owner),marker=r.previews[0];const strict=owner.bufferedRestore(t);assert.equal(strict.feed(t,Buffer.from(marker)),null);assert.equal(strict.finalize(t).text,'alice@example.test');const progressive=owner.progressiveRestore(t);const chunk=progressive.feed(t,Buffer.from('prefix '));assert.equal(chunk.complete,false);assert.ok(chunk.delivered.bytes>0);let failure;try{progressive.feed(t,new Uint8Array(new SharedArrayBuffer(2)));}catch(e){failure=e;}assert.equal(failure.code,'invalid_encoding');assert.equal(failure.complete,false);assert.equal(failure.delivered.bytes,chunk.delivered.bytes);assert.equal('text' in failure,false);assert.throws(()=>progressive.feed(t,Buffer.from('later')),code('inactive_stream'));const utf16=owner.bufferedRestore(t,'utf16');const data=Uint16Array.from([...marker].map(c=>c.charCodeAt(0)));utf16.feed(t,data);assert.equal(utf16.finalize(t).text,'alice@example.test');const pending=owner.bufferedRestore(t);const p=owner.preparePolicyChanges(t,{rules:[{entityType:'EMAIL',rule:null}],choices:[]},id());owner.commit(t,p,true,id());pending.feed(t,Buffer.from('same head'));assert.equal(pending.finalize(t).text,'same head');const stale=owner.bufferedRestore(t);saved(owner,'bob@example.test');assert.throws(()=>stale.feed(t,Buffer.from('x')),code('workspace_conflict'));owner.lock(t);assert.throws(()=>utf16.reset(t),code('inactive_stream'));owner.close(t);owner=null;}finally{owner?.close(t);f.cleanup();}
});

test('installed_node_dropped_ack_reconciliation_requires_fresh_owner',()=>{
 const f=fixture();let owner;
 try{owner=f.owner();const plan=owner.prepareReview(t,[review()],id());const candidate=plan.heads(t).candidate;const marker=plan.previews(t)[0].text;const request=id();owner.commit(t,plan,true,request); // Drop the host-visible return: synthetic lost reply, not a power cut.
 owner.lock(t);assert.deepEqual(owner.reconcile(t,request).requestId,Array.from(request));assert.throws(()=>owner.head(t),code('reopen_required'));owner.close(t);owner=null;const {password,operationId,requestId,...config}=f.config;assert.deepEqual(NativeWorkspace.reconcileExisting(config,request).requestId,Array.from(request));owner=open(f.config,candidate);assert.equal(owner.restore(t,marker),'alice@example.test');assert.throws(()=>owner.commit(t,plan,true,id()),code('foreign_change'));owner.close(t);owner=null;}finally{owner?.close(t);f.cleanup();}
});

test('installed_node_errors_are_structured_and_content_free',()=>{
 const f=fixture();let owner;
 try{owner=f.owner();const input=review();input.scannerFindings[0].range.end=1;let caught;try{owner.prepareReview(t,[input],id());}catch(e){caught=e;}assert.equal(caught.code,'invalid_finding');assert.equal(caught.findingIndex,0);assert.equal(caught.message.includes('alice@example.test'),false);assert.equal(JSON.stringify(caught).includes('alice@example.test'),false);const plan=owner.prepareRecovery(t,id());assert.equal(plan.previews(t).length,0);owner.commit(t,plan,true,id());assert.ok(owner.counts(t).operations>=2);assert.equal(owner.historyCoverage(t).recordedHistoryOnly,true);assert.ok(owner.operationsPage(t,0,10).total>=2);owner.close(t);owner=null;}finally{owner?.close(t);f.cleanup();}
});


test('installed_node_authenticated_import_uses_fresh_destination',()=>{
 if(!process.env.DATAFOG_NODE_AUTHENTICATED_FIXTURE)throw Error('run installed verifier to provision authenticated synthetic source fixture');
 const source=JSON.parse(readFileSync(process.env.DATAFOG_NODE_AUTHENTICATED_FIXTURE,'utf8'));
 assert.equal(source.synthetic,true);
 const f=fixture();let owner;
 const options={...f.config,namespace:Uint8Array.from(source.namespace),sourceHead:source.head};
 try{
  assert.throws(()=>NativeWorkspace.importAuthenticatedBundle(options,Uint8Array.from(source.bundle),false),code('approval_required'));
  assert.equal(existsSync(options.protectedControlPath),false);
  assert.throws(()=>NativeWorkspace.importAuthenticatedBundle({...options,sourceHead:{...source.head,commitId:Array.from(id())}},Uint8Array.from(source.bundle),true),code('binding_mismatch'));
  assert.equal(existsSync(options.protectedControlPath),false);
  for(const invalid of source.invalidTimestampBundles){
    assert.throws(()=>NativeWorkspace.importAuthenticatedBundle(options,Uint8Array.from(invalid.bundle),true),code('invalid_snapshot'));
    assert.equal(existsSync(options.protectedControlPath),false);
  }
  owner=NativeWorkspace.importAuthenticatedBundle(options,Uint8Array.from(source.bundle),true);
  assert.notDeepEqual(owner.head(t),source.head);
  assert.equal(owner.restore(t,source.marker),source.text);
  assert.ok(Number.isSafeInteger(source.maxOperationTimeMs));
  const timed=owner.operationsPage(t,0,10).records.find(row=>row.occurredAtMs!==null);
  assert.equal(timed.occurredAtMs,source.maxOperationTimeMs);
  assert.equal(typeof timed.occurredAtMs,'number');
  const head=owner.head(t);owner.close(t);owner=null;
  const {sourceHead,...config}=options;
  owner=open(config,head);assert.equal(owner.restore(t,source.marker),source.text);owner.close(t);owner=null;
  assert.throws(()=>NativeWorkspace.importAuthenticatedBundle(options,Uint8Array.from(source.bundle),true),code('destination_occupied'));
 }finally{owner?.close(t);f.cleanup();}
});

test('installed_node_abandoned_handles_release_native_capacity',async()=>{
 assert.equal(typeof global.gc,'function','installed proof must enable explicit GC');
 const f=fixture();let owner=f.owner();
 try{
  const pending=owner.prepareRecovery(t,id());
  const unchanged=owner.head(t);
  assert.throws(()=>owner.prepareRecovery(t,id()),code('resource_limit'));
  assert.throws(()=>owner.commit(t,pending,false,id()),code('approval_required'));
  assert.throws(()=>owner.prepareReview(t,[review()],id()),code('resource_limit'));
  assert.deepEqual(owner.head(t),unchanged);
  assert.ok(pending.heads(t).candidate);
  pending.discard(t);
  const consumed=owner.prepareRecovery(t,id());
  owner.commit(t,consumed,true,id());
  const next=owner.prepareRecovery(t,id());next.discard(t);
  assert.throws(()=>consumed.previews(t),code('inactive_plan'));
  async function abandonAndRecover(make,release,capacity){
   const weak=[];
   // Fill the bounded native registry while keeping no JavaScript handles.
   for(let i=0;i<capacity;i++){let child=make();weak.push(new WeakRef(child));child=null;}
   let recovered=[];
   // GC and finalization run on separate turns. Retry capacity, rather than
   // assuming that a finalizer must have run immediately after global.gc().
   for(let attempt=0;attempt<100;attempt++){
    await new Promise(resolve=>setImmediate(resolve));global.gc();
    await new Promise(resolve=>setImmediate(resolve));
    try{while(recovered.length<capacity)recovered.push(make());}catch(e){assert.equal(e.code,'resource_limit');}
    if(recovered.length===capacity)break;
   }
   assert.equal(weak.filter(ref=>ref.deref()!==undefined).length,0,'abandoned wrappers must be collected');
   assert.equal(recovered.length,capacity,'all abandoned native handle slots must become reusable');
   for(const child of recovered)release(child);
  }
  await abandonAndRecover(()=>owner.prepareRecovery(t,id()),p=>p.discard(t),1);
  await abandonAndRecover(()=>owner.bufferedRestore(t),s=>s.close(t),32);
  const child=owner.prepareRecovery(t,id()),stream=owner.bufferedRestore(t),weakOwner=new WeakRef(owner);
  owner=null;
  for(let i=0;i<3;i++){await new Promise(resolve=>setImmediate(resolve));global.gc();}
  assert.ok(weakOwner.deref(),'retained child deliberately retains owner');
  assert.ok(child.heads(t).candidate);
  weakOwner.deref().close(t);
  assert.throws(()=>child.previews(t),code(['inactive_plan','closed_workspace']));
  assert.throws(()=>stream.feed(t,Buffer.from('late')),code('inactive_stream'));
 }finally{owner?.close(t);f.cleanup();}
});
