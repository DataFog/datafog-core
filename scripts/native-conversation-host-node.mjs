// Actual installed native SDK/store; controlled callbacks and site witnesses are test-only.
import assert from "node:assert/strict";
import {isDeepStrictEqual} from "node:util";
import {randomBytes,createHash} from "node:crypto";
import {readFileSync,writeFileSync,renameSync} from "node:fs";
import {mkdtemp,readFile,rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import path from "node:path";
import {fork} from "node:child_process";
import {setTimeout as pause} from "node:timers/promises";
import {DatabaseSync} from "node:sqlite";
import {Workspace,DataFogError} from "@datafog/node";
import {EncryptedWorkspaceStore,HostFailure} from "./node-workspace-host.mjs";
const id=n=>new Uint8Array(16).fill(n),hex=value=>Buffer.from(value).toString("hex"),sha=value=>createHash("sha256").update(value).digest("hex");
let trace={transportCalls:0,resolutionPreparations:0};
const operation=n=>({id:id(n),occurredAtMs:n});
function snapshot(w){return {head:w.head(),namespaceHex:hex(w.namespace()),bundleHex:hex(w.exportBundle()),policy:w.policy(),counts:w.counts(),operations:w.operations(),historyCoverage:w.historyCoverage()};}
function open(store,scope){const saved=store.load(scope);try{return Workspace.openBundle(saved.bundle);}finally{saved.bundle.fill(0);}}
function state(store,scope){const w=open(store,scope);try{return snapshot(w);}finally{w.close();}}
function errorValue(error){assert(error instanceof HostFailure||error instanceof DataFogError);assert.equal(error.message,error.code);return {type:error instanceof HostFailure?"host":"core",code:error.code,message:error.message,...(error instanceof DataFogError?{inputIndex:error.inputIndex??null,findingIndex:error.findingIndex??null,markerIssues:error.markerIssues??[],reviewConflicts:error.reviewConflicts??[],delivered:error.delivered===undefined?null:error.delivered,complete:error.complete===undefined?null:error.complete}: {})};}
function rowDigest(database,scope){const db=new DatabaseSync(database);try{return sha(JSON.stringify(db.prepare("SELECT * FROM workspaces WHERE tenant=? AND workspace=?").get(scope.tenantId,scope.workspaceId),(_,value)=>typeof value==="bigint"?String(value):value));}finally{db.close();}}
function prepare(w,fixture,which){const [key,number]={a:["a",3],b:["b",5],followup:["followup",11]}[which];const plan=w.prepareConversationSend([fixture.inputs[key]],operation(number));return {plan,intent:plan.sendIntents()[0],output:plan.preview().text};}
function persist(store,scope,w,plan,{fault,expected,checkpoint}={}){
 const heads=plan.heads(),parent=expected??heads.expectedHead,bytes=plan.exportCandidateBundle();
 const result={expectedHead:parent,candidateHead:heads.candidateHead,candidateBundleHex:hex(bytes),saveCalls:1,activated:false};
 try{
  result.acknowledgment=store.saveIfHeadMatches(scope,parent,heads.candidateHead,bytes,phase=>{checkpoint?.(phase);fault?.(phase);});
  w.activateSaved(plan,heads.expectedHead,heads.candidateHead);Object.assign(result,{ok:true,activated:true,localHead:w.head()});
 }catch(error){Object.assign(result,{ok:false,error:errorValue(error),localHead:w.head()});}
 finally{bytes.fill(0);}
 return result;
}
function closed(value,keys){if(!value||typeof value!=="object"||Array.isArray(value)||![Object.prototype,null].includes(Object.getPrototypeOf(value)))return false;const properties=Object.getOwnPropertyDescriptors(value);return Reflect.ownKeys(properties).length===keys.length&&keys.every(key=>properties[key]?.enumerable&&Object.hasOwn(properties[key],"value"));}
function byteId(value){return Array.isArray(value)&&value.length===16&&Reflect.ownKeys(value).length===17&&Array.from({length:16},(_,index)=>Object.getOwnPropertyDescriptor(value,String(index))).every(item=>item?.enumerable&&Object.hasOwn(item,"value")&&Number.isInteger(item.value)&&item.value>=0&&item.value<=255);}
function validScope(value){return closed(value,["tenantId","workspaceId"])&&Object.values(value).every(item=>typeof item==="string"&&item.length>0);}
function association(binding,witness,callback){
 if(!closed(binding,["scope","preparationId","inputIndex","logicalId","surface","payloadSha256"])||!validScope(binding.scope)||!byteId(binding.preparationId)||!byteId(binding.logicalId)||!Number.isInteger(binding.inputIndex)||binding.inputIndex<0||binding.inputIndex>=32||typeof binding.surface!=="string"||!binding.surface.length||typeof binding.payloadSha256!=="string"||!/^[a-f0-9]{64}$/.test(binding.payloadSha256))return false;
 if(!closed(witness,["scope","logicalId","surface","siteId"])||typeof witness.siteId!=="string"||!witness.siteId.length)return false;
 if(!["scope","logicalId","surface"].every(key=>isDeepStrictEqual(witness[key],binding[key])))return false;
 if(!closed(callback,[...Object.keys(binding),"siteId","kind"])||typeof callback.kind!=="string"||!["observed_sent","definitely_not_sent","uncertain"].includes(callback.kind))return false;
 return isDeepStrictEqual(callback,{...binding,siteId:witness.siteId,kind:callback.kind});
}
function dispatch(store,scope,fixture,intent,output,kind="observed_sent",callbackChanges){
 const durable=open(store,scope);let before;
 try{assert.equal(durable.sendIntentStatus(intent),"pending");before=snapshot(durable);}finally{durable.close();}
 const binding={scope:{...scope},preparationId:[...intent.preparationId],inputIndex:intent.inputIndex,logicalId:[...id(10)],surface:fixture.surface,payloadSha256:sha(Buffer.from(output))};
 const witness={scope:{...scope},logicalId:binding.logicalId,surface:fixture.surface,siteId:fixture.siteA};
 trace.transportCalls++;const received={...binding,siteId:witness.siteId,kind};
 return {attempt:binding,siteVerification:witness,durableBeforeCallback:before,...(callbackChanges?{callbacks:callbackChanges.map(change=>({...received,...change}))}:{received})};
}
function resolution(w,evidence,intent,number,notSent=false){assert(association(evidence.attempt,evidence.siteVerification,evidence.received));assert.equal(evidence.received.kind,notSent?"definitely_not_sent":"observed_sent");const plan=notSent?w.prepareDefinitelyNotSent(intent,id(10),operation(number)):w.prepareObservedSend(intent,id(10),evidence.received.surface,evidence.received.siteId,operation(number));trace.resolutionPreparations++;return plan;}
function scopedError(w,text,logical=10){try{w.restoreInConversation(text,id(logical));}catch(error){return errorValue(error);}throw new Error("scoped rejection missing");}
function verifyAssociation(evidence){
 assert(association(evidence.attempt,evidence.siteVerification,evidence.received));
 const mutations=[
  ["boolean_index",(_b,_w,c)=>{c.inputIndex=false;}], ["float_index",(_b,_w,c)=>{c.inputIndex=.5;}],
  ["nonstring_hash",(_b,_w,c)=>{c.payloadSha256=0;}], ["nonstring_surface",(_b,_w,c)=>{c.surface=false;}],
  ["nonstring_witness_site",(_b,w)=>{w.siteId=false;}], ["extra_callback",(_b,_w,c)=>{c.extra=true;}],
  ["extra_witness",(_b,w)=>{w.extra=true;}], ["boolean_nested_id",(_b,_w,c)=>{c.logicalId[0]=true;}],
  ["float_nested_id",(_b,_w,c)=>{c.preparationId[0]=3.5;}], ["boolean_witness_scope",(_b,w)=>{w.scope.tenantId=false;}],
  ["float_witness_scope",(_b,w)=>{w.scope.workspaceId=1.5;}],
 ];
 for(const [name,mutate] of mutations){const [binding,witness,callback]=structuredClone([evidence.attempt,evidence.siteVerification,evidence.received]);mutate(binding,witness,callback);assert(!association(binding,witness,callback),name);}
 return {passed:true,rejected:mutations.length,cases:mutations.map(([name])=>name)};
}
function child(config){
 const key=Buffer.from(config.keyHex,"hex"),store=new EncryptedWorkspaceStore(config.database,()=>key),scope=config.scope,events=[];let w,plan,confirmation;
 const finish=value=>{process.send(value,error=>{if(error)process.exitCode=1;process.disconnect();});};
 try{
  w=open(store,scope);let intent,output,evidence;
  if(config.role==="confirmation"){intent=config.intent;evidence=config.evidence;plan=resolution(w,evidence,intent,config.resolutionByte);}
  else{({plan,intent,output}=prepare(w,config.fixture,config.which??"a"));}
  const heads=plan.heads(),candidateHex=hex(plan.exportCandidateBundle());
  const boundary=(stage,subject)=>phase=>{if(config.crashStage===stage&&config.crashAt===phase){const current=subject.heads();writeFileSync(config.phaseFile+".tmp",JSON.stringify({stage,boundary:phase,events,trace:{...trace},expectedHead:current.expectedHead,candidateHead:current.candidateHead,candidateBundleHex:hex(subject.exportCandidateBundle())}));renameSync(config.phaseFile+".tmp",config.phaseFile);Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0);}};
  const save=()=>{
   try{
    const saved=persist(store,scope,w,plan,{checkpoint:boundary(config.role==="confirmation"?"confirmation":"pending",plan)});
    if(config.role==="confirmation")return {save:saved,transportEvents:[],resolutionEvidence:evidence,trace:{...trace}};
    if(!saved.ok)return {save:saved,transportEvents:[],confirmation:null,trace:{...trace}};
    evidence=dispatch(store,scope,config.fixture,intent,output);events.push(evidence);
    confirmation=resolution(w,evidence,intent,(config.which??"a")==="a"?4:6);
    return {save:saved,transportEvents:events,confirmation:persist(store,scope,w,confirmation,{checkpoint:boundary("confirmation",confirmation)}),trace:{...trace}};
   }finally{w.close();store.close();key.fill(0);}
  };
  if(config.crashStage){save();process.exitCode=1;process.disconnect();return;}
  process.send({ready:true,parent:heads.expectedHead,candidateHead:heads.candidateHead,candidateBundleHex:candidateHex});
  process.once("message",message=>{assert.equal(message,"save");finish(save());});
 }catch{w?.close();store.close();key.fill(0);process.exitCode=1;process.disconnect();}
}
function message(child){return new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(new Error("conversation child timeout")),20000);child.once("message",value=>{clearTimeout(timer);resolve(value);});child.once("error",error=>{clearTimeout(timer);reject(error);});child.once("exit",()=>{clearTimeout(timer);reject(new Error("conversation child exited before result"));});});}
function start(config,children){const process=fork(new URL(import.meta.url),["--child"],{stdio:["ignore","ignore","ignore","ipc"],serialization:"advanced"});children.push(process);const exit=new Promise(resolve=>process.once("exit",(code,signal)=>resolve({code,signal})));const ready=config.crashStage?undefined:message(process);process.send(config);return {process,ready,exit};}
async function run(fixture,oracle){
 const directory=await mkdtemp(path.join(tmpdir(),"datafog-conversation-host-")),children=[],cases=[];
 try{
  for(const scenario of fixture.cases){
   trace={transportCalls:0,resolutionPreparations:0};
   const name=scenario.id,database=path.join(directory,name+".sqlite"),scope={...fixture.hostScopes.node},key=randomBytes(32);
   const store=new EncryptedWorkspaceStore(database,owner=>owner.tenantId===scope.tenantId&&owner.workspaceId===scope.workspaceId?key:undefined);
   const initial=name==="existing-chat-pause"?"twoConfirmed":"seed",bootstrap=Uint8Array.from(Buffer.from(oracle.states[initial].bundleHex,"hex"));store.initialize(scope,bootstrap);bootstrap.fill(0);
   const result={id:name,before:state(store,scope),steps:[]},record=(call,value)=>result.steps.push({call,value});
   let w,plan,confirmation,foreign,foreignKey;
   try{
    w=open(store,scope);
    if(name.startsWith("process-")||name.startsWith("crash-")){
     const config={fixture,database,scope,keyHex:hex(key)};let configs;
     if(name==="process-confirmation-race"){
      const pending=prepare(w,fixture,"a");record("pending-saved",persist(store,scope,w,pending.plan));const evidence=dispatch(store,scope,fixture,pending.intent,pending.output);record("transport-observed",evidence);
      configs=[4,8].map(resolutionByte=>({...config,role:"confirmation",resolutionByte,intent:pending.intent,evidence}));
     }else if(name==="process-pending-race")configs=["a","b"].map(which=>({...config,role:"pending",which}));
     else{
      const stage=name.startsWith("crash-pending-")?"pending":"confirmation",phase=name.endsWith("before")?"before_commit":"after_commit",phaseFile=path.join(directory,name+".phase");
      const active=start({...config,role:"pending",which:"a",crashStage:stage,crashAt:phase,phaseFile},children);let evidence;
      for(let attempt=0;attempt<2000;attempt++){try{evidence=JSON.parse(await readFile(phaseFile,"utf8"));}catch(error){if(error.code!=="ENOENT")throw error;}if(evidence)break;if(active.process.exitCode!==null||active.process.signalCode!==null)throw new Error("child exited before checkpoint");await pause(10);}
      assert(evidence,"real child reached persistence boundary");record("child-at-boundary",evidence);active.process.kill("SIGKILL");record("killed",await active.exit);record(scenario.steps.at(-1),state(store,scope));result.after=state(store,scope);result.trace={...trace};cases.push(result);continue;
     }
     const pending=configs.map(config=>start(config,children));record("two-processes-ready",await Promise.all(pending.map(item=>item.ready)));
     const outcomes=await Promise.all(pending.map(async item=>{const response=message(item.process);item.process.send("save");const result=await response;result.processExit=await item.exit;assert.deepEqual(result.processExit,{code:0,signal:null});return result;}));
     record("race",outcomes);record("reopen-exact-winner",state(store,scope));result.after=state(store,scope);result.trace={...trace};cases.push(result);continue;
    }
    const prepared=prepare(w,fixture,name==="existing-chat-pause"?"followup":"a");({plan}=prepared);const {intent,output}=prepared;
    if(name==="pending-save-rollback"){
     record("save-failed",persist(store,scope,w,plan,{fault:phase=>{if(phase==="before_commit")throw new HostFailure("storage_failure");}}));record("no-dispatch",{transportEvents:[],resolutionCalls:0});record("reopen-original",state(store,scope));
    }else{
     if(name==="existing-chat-pause")record("two-confirmed",snapshot(w));
     record(name==="existing-chat-pause"?"followup-saved":"pending-saved",persist(store,scope,w,plan));
     if(name==="existing-chat-pause"){
      const paused=()=>{const reopened=open(store,scope);try{return {snapshot:snapshot(reopened),plain:scopedError(reopened,"old plain"),old:scopedError(reopened,oracle.plans.sendA.previews[0].text),other:reopened.restoreInConversation(oracle.plans.otherSend.previews[0].text,id(11))};}finally{reopened.close();}};
      record("reopen-paused",paused());const recovered=w.prepareRecoveryWithOperation(operation(13));record("recovery-saved",persist(store,scope,w,recovered));record("still-paused",paused());
      const evidence=dispatch(store,scope,fixture,intent,output,"definitely_not_sent");const stopped=resolution(w,evidence,intent,14,true);record("not-sent-saved",{observation:evidence,save:persist(store,scope,w,stopped)});
      const reopened=open(store,scope);try{record("reopen-resumed",{snapshot:snapshot(reopened),old:reopened.restoreInConversation(oracle.plans.sendA.previews[0].text,id(10)),otherNotIssued:scopedError(reopened,"👋 "+oracle.plans.otherSend.previews[0].text)});}finally{reopened.close();}
     }else{
      const kind=name==="definitely-not-sent"?"definitely_not_sent":name==="uncertain-transport"?"uncertain":"observed_sent";
      if(["callback-mismatch","context-and-tenant-isolation"].includes(name)){
       let changes=[{preparationId:[...id(99)]},{logicalId:[...id(11)]},{siteId:"wrong_site"},{inputIndex:false},{payloadSha256:0},{surface:false}];let foreignBefore;
       if(name==="context-and-tenant-isolation"){
        const foreignScope={...scope,tenantId:"synthetic_foreign"};foreignKey=randomBytes(32);foreign=new EncryptedWorkspaceStore(database,owner=>owner.tenantId===foreignScope.tenantId&&owner.workspaceId===foreignScope.workspaceId?foreignKey:undefined);const bytes=Uint8Array.from(Buffer.from(oracle.states.seed.bundleHex,"hex"));foreign.initialize(foreignScope,bytes);bytes.fill(0);foreignBefore={scope:foreignScope,digest:rowDigest(database,foreignScope)};
        changes=[{scope:{...scope,tenantId:"synthetic_foreign"}},{scope:{...scope,workspaceId:"synthetic_foreign"}}];
       }
       const evidence=dispatch(store,scope,fixture,intent,output,kind,changes);for(const callback of evidence.callbacks)assert(!association(evidence.attempt,evidence.siteVerification,callback));
       record(name==="context-and-tenant-isolation"?"foreign-callbacks":"callbacks-received",{...evidence,transportCalls:trace.transportCalls,resolutionCalls:trace.resolutionPreparations});
       if(foreignBefore)record("foreign-store-unchanged",{before:foreignBefore.digest,after:rowDigest(database,foreignBefore.scope),snapshot:state(foreign,foreignBefore.scope)});
       if(name==="callback-mismatch")record("no-confirmation",{resolutionCalls:0});else record("scoped-unknown",scopedError(w,"plain",99));
       record(name==="context-and-tenant-isolation"?"unchanged-pending":"reopen-pending",state(store,scope));
      }else if(name==="uncertain-transport"){const evidence=dispatch(store,scope,fixture,intent,output,kind);record("transport-uncertain",evidence);record("reopen-pending",state(store,scope));}
      else{
       const evidence=dispatch(store,scope,fixture,intent,output,kind);
       record(kind==="definitely_not_sent"?"transport-not-sent":"transport-observed",evidence);confirmation=resolution(w,evidence,intent,kind==="definitely_not_sent"?7:4,kind==="definitely_not_sent");
       if(name==="full-parent-conflict"){
        const parent=confirmation.heads().expectedHead,digest=rowDigest(database,scope);record("wrong-revision",persist(store,scope,w,confirmation,{expected:{...parent,revision:String(BigInt(parent.revision)+1n)}}));record("wrong-commit-id",persist(store,scope,w,confirmation,{expected:{...parent,commitId:[...id(99)]}}));record("unchanged-pending",{snapshot:state(store,scope),rowBeforeSha256:digest,rowAfterSha256:rowDigest(database,scope)});
       }else{
        const fault=name==="confirmation-save-rollback"?phase=>{if(phase==="before_commit")throw new HostFailure("storage_failure");}:name==="confirmation-lost-ack"?phase=>{if(phase==="after_commit")throw new HostFailure("save_ack_unknown");}:undefined;
        const saved=persist(store,scope,w,confirmation,{fault});record(name==="confirmation-save-rollback"?"confirmation-failed":name==="confirmation-lost-ack"?"confirmation-uncertain":kind==="definitely_not_sent"?"resolution-saved":"confirmation-saved",saved);
        if(name==="confirmation-save-rollback"){
         record("reopen-pending",state(store,scope));const reopened=open(store,scope);try{const retry=resolution(reopened,evidence,intent,4);record("retry-metadata-only",{save:persist(store,scope,reopened,retry),transportCalls:1,received:evidence.received});}finally{reopened.close();}
        }
        record(kind==="definitely_not_sent"?"reopen-not-sent":"reopen-confirmed",state(store,scope));
        if(name==="confirmation-lost-ack"){
         const reopened=open(store,scope);try{try{reopened.prepareObservedSend(intent,id(10),fixture.surface,fixture.siteA,operation(15));throw new Error("replay accepted");}catch(error){record("replay-rejected",errorValue(error));}}finally{reopened.close();}
        }
       }
      }
     }
    }
    result.after=state(store,scope);result.trace={...trace};cases.push(result);
   }finally{w?.close();foreign?.close();foreignKey?.fill(0);store.close();key.fill(0);}
  }
  return {schemaVersion:1,profile:fixture.profile,sdk:"node",cases,associationChecks:verifyAssociation(cases[0].steps[1].value)};
 }finally{await Promise.all(children.map(child=>child.exitCode!==null||child.signalCode!==null?undefined:new Promise(resolve=>{child.once("exit",resolve);child.kill("SIGKILL");})));await rm(directory,{recursive:true,force:true});}
}
if(process.argv[2]==="--child")process.once("message",child);else process.stdout.write(JSON.stringify(await run(JSON.parse(readFileSync(process.argv[2],"utf8")),JSON.parse(readFileSync(process.argv[3],"utf8")))));
