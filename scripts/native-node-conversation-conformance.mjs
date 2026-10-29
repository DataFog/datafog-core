import assert from "node:assert/strict";
import {mkdtemp,rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import path from "node:path";
import {randomBytes} from "node:crypto";
import {Worker,isMainThread,parentPort,workerData} from "node:worker_threads";
import * as installed from "@datafog/node";
import {verifyWorkspaceConversations} from "./workspace-conversation-conformance.mjs";
import {EncryptedWorkspaceStore,HostFailure} from "./node-workspace-host.mjs";

if(!isMainThread) {
  const workspace=installed.Workspace.openBundle(workerData.bundle);
  try {
    const stream=workspace.progressiveRestoreInConversation(workerData.logicalId);
    try {
      const chunk=stream.feed(new TextEncoder().encode(workerData.marker)),tail=stream.finalize();
      parentPort.postMessage({head:workspace.head(),text:chunk.text+tail.text,status:workspace.sendIntentStatus(workerData.intent)});
    } finally {stream.close();}
  } finally {workspace.close();workerData.bundle.fill(0);}
}

/** Actual installed native calls plus the existing encrypted CAS store. No provider claim. */
export async function verifyNativeConversations(api) {
  const shared=await verifyWorkspaceConversations({...api,init:async()=>{}});
  const id=value=>new Uint8Array(16).fill(value),op=value=>({id:id(value),occurredAtMs:value});
  const context=(value,siteId=null)=>({id:id(value),surface:"synthetic_chat",siteId,siteIdByteBudget:128});
  const input=(text,mark=false)=>({text,scannerFindings:[],userSelections:mark?[{entityType:"PERSON",matchedText:text,range:{unit:"utf8_bytes",start:0,end:Buffer.byteLength(text)}}]:[]});
  const reject=(action,code,index)=>assert.throws(action,error=>error instanceof api.DataFogError && error.code===code && error.message===code && (index===undefined || error.inputIndex===index));
  const save=(workspace,plan)=>{const heads=plan.heads();workspace.activateSaved(plan,heads.expectedHead,heads.candidateHead);};
  const workspace=api.Workspace.createWithOperation(id(20),op(21));
  const before=workspace.exportBundle();
  try {
    const definition=context(22),review=input("Native Synthetic 👋",true);
    let calls=0;
    const getter={...definition};Object.defineProperty(getter,"surface",{enumerable:true,get(){calls++;return "private";}});
    reject(()=>workspace.prepareConversationSend([{input:review,conversation:definition},{input:review,conversation:getter}],op(23)),"invalid_conversation",1);
    const accessorId=Array(16).fill(22);Object.defineProperty(accessorId,"0",{enumerable:true,get(){calls++;return 22;}});
    reject(()=>workspace.prepareConversationSend([{input:review,conversation:{...definition,id:accessorId}}],op(23)),"invalid_conversation",0);
    for(const value of [null,{},[1],Array(16).fill(true),Array(16),new Uint8Array(15)]) {
      reject(()=>workspace.prepareConversationSend([{input:review,conversation:{...definition,id:value}}],op(23)),value instanceof Uint8Array?"invalid_operation_id":"invalid_conversation",0);
    }
    reject(()=>workspace.prepareConversationSend([{input:review,conversation:definition},{input:{...review,corrections:null},conversation:definition}],op(23)),"invalid_finding",1);
    reject(()=>workspace.prepareConversationSend([{input:{...review,userSelections:[{...review.userSelections[0],confidence:NaN}]},conversation:definition}],op(23)),"invalid_finding",0);
    const operation=op(23);Object.defineProperty(operation,"occurredAtMs",{enumerable:true,get(){calls++;return 23;}});
    reject(()=>workspace.prepareConversationSend([{input:review,conversation:definition}],operation),"invalid_operation");
    assert.equal(calls,0);assert.deepEqual(workspace.exportBundle(),before);
    const plans=Array.from({length:32},(_,index)=>workspace.prepareConversationSend([{input:input("plain"),conversation:definition}],op(40+index)));
    reject(()=>workspace.prepareConversationSend([{input:input("plain"),conversation:definition}],op(80)),"resource_limit");
    plans.pop().discard();workspace.prepareConversationSend([{input:input("plain"),conversation:definition}],op(80)).discard();
    for(const plan of plans)plan.discard();
    const first=workspace.prepareConversationSend([{input:review,conversation:definition}],op(23));
    const retained=first.sendIntents(),canonical=structuredClone(retained[0]),marker=first.preview().text;
    retained[0].preparationId.fill(0);retained[0].inputIndex=31;
    definition.id.fill(0);definition.surface="changed";review.text="changed";
    assert.deepEqual(first.sendIntents()[0],canonical,"intent views and input contexts are owned");
    const invalidIntent={...canonical};Object.defineProperty(invalidIntent,"inputIndex",{enumerable:true,get(){calls++;return 0;}});
    reject(()=>workspace.sendIntentStatus(invalidIntent),"invalid_send_intent");assert.equal(calls,0);
    const heads=first.heads(),other=api.Workspace.openBundle(before);
    try {reject(()=>other.activateSaved(first,heads.expectedHead,heads.candidateHead),"workspace_conflict");}finally{other.close();}
    save(workspace,first);reject(()=>first.sendIntents(),"inactive_plan");
    const confirmed=workspace.prepareObservedSend(canonical,id(22),"synthetic_chat","thread_native",op(24));save(workspace,confirmed);
    const view=workspace.conversation(id(22));view.conversation.id.fill(0);view.conversation.siteId="changed";
    assert.equal(workspace.conversation(id(22)).conversation.siteId,"thread_native");
    const site=workspace.conversationForSite("synthetic_chat","thread_native");site.fill(0);
    assert.deepEqual(workspace.conversationForSite("synthetic_chat","thread_native"),id(22));
    const utf16=workspace.bufferedRestoreInConversation(id(22),"utf16");
    for(let offset=0;offset<marker.length;offset++)utf16.feedUtf16(new Uint16Array([marker.charCodeAt(offset)]));
    assert.equal(utf16.finalize().text,"Native Synthetic 👋");utf16.close();
    const live=workspace.progressiveRestoreInConversation(id(22)),buffered=workspace.bufferedRestoreInConversation(id(22));
    const noop=workspace.prepare("plain",[],id(25));save(workspace,noop);
    assert.equal(live.feed(new TextEncoder().encode("safe ")).text,"safe ");
    const next=workspace.prepareConversationSend([{input:input(marker),conversation:context(22,"thread_native")}],op(26));const nextIntent=next.sendIntents()[0];save(workspace,next);
    assert.throws(()=>live.feed(new TextEncoder().encode(marker)),error=>error.code==="workspace_conflict" && error.complete===false && error.delivered.bytes===5);
    reject(()=>buffered.finalize(),"workspace_conflict");live.close();buffered.close();
    const notSent=workspace.prepareDefinitelyNotSent(nextIntent,id(22),op(27));
    notSent.discard();
  } finally {workspace.close();before.fill(0);}

  const directory=await mkdtemp(path.join(tmpdir(),"datafog-native-conversations-")),key=randomBytes(32),scope={tenantId:"synthetic_tenant",workspaceId:"synthetic_conversation"};
  const store=new EncryptedWorkspaceStore(path.join(directory,"saved.sqlite"),()=>key);
  let local;
  const open=()=>{const saved=store.load(scope);try{return api.Workspace.openBundle(saved.bundle);}finally{saved.bundle.fill(0);}};
  const persist=(workspace,plan,fault)=>{const heads=plan.heads(),bytes=plan.exportCandidateBundle();try{const ack=store.saveIfHeadMatches(scope,heads.expectedHead,heads.candidateHead,bytes,fault);workspace.activateSaved(plan,heads.expectedHead,ack);}finally{bytes.fill(0);}};
  try {
    local=api.Workspace.createWithOperation(id(100),op(101));
    const initial=local.exportBundle();try{store.initialize(scope,initial);}finally{initial.fill(0);}
    const definition=context(102),plan=local.prepareConversationSend([{input:input("Encrypted Synthetic",true),conversation:definition}],op(103));
    const intent=plan.sendIntents()[0],marker=plan.preview().text;
    assert.throws(()=>persist(local,plan,phase=>{if(phase==="before_commit")throw new HostFailure("storage_failure");}),error=>error.code==="storage_failure");
    const rolledBack=open();try{reject(()=>rolledBack.conversation(id(102)),"conversation_unknown");}finally{rolledBack.close();}
    persist(local,plan);
    const pending=open();try{reject(()=>pending.restoreInConversation("plain",id(102)),"conversation_pending");}finally{pending.close();}
    const confirmed=local.prepareObservedSend(intent,id(102),"synthetic_chat","thread_saved",op(104));
    assert.throws(()=>persist(local,confirmed,phase=>{if(phase==="after_commit")throw new HostFailure("save_ack_unknown");}),error=>error.code==="save_ack_unknown");
    assert.equal(local.sendIntentStatus(intent),"pending","lost acknowledgment does not locally activate");local.close();local=open();
    assert.equal(local.sendIntentStatus(intent),"observed_sent");assert.equal(local.counts().confirmedHandoffs,1);
    assert.equal(local.restoreInConversation(marker,id(102)),"Encrypted Synthetic");
    reject(()=>local.prepareObservedSend(intent,id(102),"synthetic_chat","thread_saved",op(105)),"invalid_send_intent");
    const worker=new Worker(new URL(import.meta.url),{workerData:{bundle:local.exportBundle(),logicalId:id(102),marker,intent}});
    try {
      const result=await new Promise((resolve,reject)=>{worker.once("message",resolve);worker.once("error",reject);worker.once("exit",code=>{if(code!==0)reject(new Error(`conversation worker exit ${code}`));});});
      assert.equal(result.text,"Encrypted Synthetic");assert.equal(result.status,"observed_sent");assert.deepEqual(result.head,local.head());
      assert.equal(local.restoreInConversation(marker,id(102)),"Encrypted Synthetic");
    } finally {await worker.terminate();}
  } finally {local?.close();store.close();key.fill(0);await rm(directory,{recursive:true,force:true});}
  return {shared,ownedTransport:true,boundedPlans:true,scopedChildren:true,encryptedRollback:true,lostAcknowledgmentReopen:true,independentWorker:true,scope:"synthetic observations through installed native Node; no actual provider send or production host approval"};
}
