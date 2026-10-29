// Synthetic-only conformance helper, not a site adapter or production send UI.
import * as api from "./vendor/package/index.js";
import {createVault,openVault,encryptCandidate,createWorkspaceRecord,randomId} from "./vault.js";
import {saveRecord,readRecords} from "./storage.js";
import {exportBackup,openBackup} from "./backup.js";
import {HostError} from "./rpc.js";
const operation=()=>({id:randomId(),occurredAtMs:Date.now()});
let transportCalls=0;
const check=(condition,label)=>{if(!condition)throw new Error(label);};
const reject=(call,code)=>{let failure;try{call();}catch(error){failure=error;}check(failure?.code===code,`expected ${code}`);return failure;};
function input(text,name) {
  const start=name===undefined?-1:text.indexOf(name),prefix=start<0?0:new TextEncoder().encode(text.slice(0,start)).length;
  return {text,scannerFindings:[],userSelections:name===undefined?[]:[{entityType:"PERSON",matchedText:name,range:{unit:"utf8_bytes",start:prefix,end:prefix+new TextEncoder().encode(name).length}}]};
}
async function session(credential,method="password") {
  await api.init();const saved=await readRecords(),opened=await openVault(credential,saved.vault,saved.workspace,undefined,method);
  try{return {vault:saved.vault,record:saved.workspace,dataKey:opened.dataKey,workspace:api.Workspace.openBundle(opened.bundle)};}
  finally {opened.bundle.fill(0);}
}
async function persist(state,plan,{abort=false,loseAck=false}={}) {
  const heads=plan.heads();
  const approved=JSON.stringify({heads,format:plan.snapshotFormat(),previews:plan.previews()});
  // This fixture supplies explicit synthetic approval, bound to that exact view.
  check(approved===JSON.stringify({heads,format:plan.snapshotFormat(),previews:plan.previews()}),"approval changed");
  const record=await encryptCandidate(state.dataKey,state.record,heads.candidateHead,plan.exportCandidateBundle());
  try {
    await saveRecord({vault:state.vault,record,expectedHead:heads.expectedHead,expectedGeneration:state.vault.accessGeneration,authorized:()=>true,abort});
    if(loseAck)throw new HostError("save_ack_unknown");
    state.workspace.activateSaved(plan,heads.expectedHead,record.head);state.record=record;
  } catch(error) {plan.discard();throw error;}
}
function acceptedSyntheticTransport(state,intent,logicalId,siteId,text) {
  // A controlled synthetic transport accepts this exact saved attempt. This is
  // never inferred from a click/copy/URL and does not verify a real provider.
  check(state.workspace.sendIntentStatus(intent)==="pending","transport before saved intent");
  check(state.workspace.conversation(logicalId).pendingIntents>0,"immutable attempt missing");
  check(typeof text==="string" && !text.includes("Synthetic"),"transport received an unprotected synthetic name");
  const received=structuredClone({intent,logicalId:Array.from(logicalId),siteId,text,surface:"synthetic_chat"});
  transportCalls++;
  return {...received,accepted:true};
}
export async function createPendingFixture(password) {
  const startCalls=transportCalls;
  await api.init();const workspace=api.Workspace.createWithOperation(randomId(),operation());
  let state;
  try {
    const created=await createVault(password,workspace.namespace(),workspace.exportBundle(),workspace.head());
    await saveRecord({...created,creating:true,authorized:()=>true});
    state={...created,workspace};
    const logicalId=randomId(),definition={id:logicalId,surface:"synthetic_chat",siteId:null,siteIdByteBudget:128};
    const first=workspace.prepareConversationSend([{input:input("First Synthetic","First Synthetic"),conversation:definition}],operation());
    const intent=first.sendIntents()[0],marker=first.preview().text;await persist(state,first);
    const observed=acceptedSyntheticTransport(state,intent,logicalId,"thread_a",marker);
    const confirmation=workspace.prepareObservedSend(intent,logicalId,observed.surface,observed.siteId,operation());
    let failed=false;try{await persist(state,confirmation,{abort:true});}catch(error){failed=error.code==="storage_failure";}
    check(failed,"confirmation transaction did not abort");check(workspace.counts().confirmedHandoffs===0,"failed save invented send");
    reject(()=>workspace.restoreInConversation("plain",logicalId),"conversation_pending");
    const backup=await exportBackup(state.vault,state.record,state.dataKey,password);
    try{return {logicalId:Array.from(logicalId),intent,marker,observed,backup:Array.from(backup),recoveryCode:created.recoveryCode,transportCalls:transportCalls-startCalls};}
    finally {backup.fill(0);}
  } finally {workspace.close();}
}
export async function inspectPendingFixture(credential,method,checkpoint) {
  const state=await session(credential,method),logicalId=new Uint8Array(checkpoint.logicalId);
  try {
    check(state.workspace.sendIntentStatus(checkpoint.intent)==="pending","restart lost pending state");
    check(state.workspace.counts().confirmedHandoffs===0,"restart invented issuance");
    reject(()=>state.workspace.restoreInConversation("old plain reply",logicalId),"conversation_pending");
    return {pending:true,profile:state.vault.profile,counts:state.workspace.counts()};
  } finally {state.workspace.close();}
}
export async function importPendingBackup(credential,method,checkpoint) {
  await api.init();const bytes=new Uint8Array(checkpoint.backup),opened=await openBackup(bytes,credential,method);bytes.fill(0);
  let workspace;
  try {
    workspace=api.Workspace.openBundle(opened.bundle);opened.bundle.fill(0);
    const old=workspace.operations(),context=workspace.conversation(new Uint8Array(checkpoint.logicalId));
    const recovery=workspace.prepareRecoveryWithOperation(operation()),heads=recovery.heads();
    const next=await createWorkspaceRecord(opened.rootKey,opened.vault,recovery.exportCandidateBundle(),heads.candidateHead);
    await saveRecord({vault:opened.vault,record:next.record,creating:true,authorized:()=>true});
    workspace.activateSaved(recovery,heads.expectedHead,next.record.head);
    check(JSON.stringify(workspace.operations().slice(0,old.length))===JSON.stringify(old),"backup removed original history");
    check(JSON.stringify(workspace.conversation(new Uint8Array(checkpoint.logicalId)))===JSON.stringify(context),"recovery changed conversation metadata");
    check(workspace.sendIntentStatus(checkpoint.intent)==="pending" && workspace.counts().confirmedHandoffs===0,"backup resolved uncertainty");
    check(workspace.operations().at(-1).kind==="recovery" && workspace.operations().length===old.length+1,"backup recovery row mismatch");
    reject(()=>workspace.restoreInConversation("plain",new Uint8Array(checkpoint.logicalId)),"conversation_pending");
    return {profile:opened.vault.profile,preserved:true};
  } finally {opened.bundle.fill(0);workspace?.close();}
}
export async function retryThenLoseAck(password,checkpoint) {
  const startCalls=transportCalls;
  const state=await session(password),logicalId=new Uint8Array(checkpoint.logicalId);
  try {
    reject(()=>state.workspace.restoreInConversation(checkpoint.marker,logicalId),"conversation_pending");
    const prior=checkpoint.observed;
    check(prior?.accepted===true && JSON.stringify(prior.intent)===JSON.stringify(checkpoint.intent) && JSON.stringify(prior.logicalId)===JSON.stringify(checkpoint.logicalId),"matching trusted observation unavailable");
    const retry=state.workspace.prepareObservedSend(checkpoint.intent,logicalId,prior.surface,prior.siteId,operation());
    await persist(state,retry); // Metadata retry only: transport is never called here.
    check(transportCalls===startCalls,"metadata retry called transport");
    check(state.workspace.counts().confirmedHandoffs===1,"metadata retry count mismatch");
    const definition=state.workspace.conversation(logicalId).conversation;
    const source=`${checkpoint.marker} and Second Synthetic`;
    const plan=state.workspace.prepareConversationSend([{input:input(source,"Second Synthetic"),conversation:definition}],operation());
    const intent=plan.sendIntents()[0],text=plan.preview().text;await persist(state,plan);
    const observed=acceptedSyntheticTransport(state,intent,logicalId,"thread_a",text);
    const confirmation=state.workspace.prepareObservedSend(intent,logicalId,observed.surface,observed.siteId,operation());
    let lost=false;try{await persist(state,confirmation,{loseAck:true});}catch(error){lost=error.code==="save_ack_unknown";}
    check(lost,"post-commit acknowledgment fault missing");
    return {logicalId:checkpoint.logicalId,intent,marker:checkpoint.marker,text,observed,transportCalls:checkpoint.transportCalls+transportCalls-startCalls};
  } finally {state.workspace.close();}
}
export async function verifyCommittedAndIsolation(password,checkpoint) {
  const startCalls=transportCalls;
  const state=await session(password),logicalId=new Uint8Array(checkpoint.logicalId);
  try {
    check(state.workspace.sendIntentStatus(checkpoint.intent)==="observed_sent","lost ack commit missing");
    check(state.workspace.counts().confirmedHandoffs===2 && state.workspace.conversation(logicalId).pendingIntents===0,"lost ack inflated or lost counts");
    check(state.workspace.restoreInConversation(checkpoint.text,logicalId)==="First Synthetic and Second Synthetic","reopen lost issued members");
    const otherId=randomId(),otherDefinition={id:otherId,surface:"synthetic_chat",siteId:null,siteIdByteBudget:128};
    const other=state.workspace.prepareConversationSend([{input:input("Other Synthetic","Other Synthetic"),conversation:otherDefinition}],operation());
    const intent=other.sendIntents()[0],otherMarker=other.preview().text;await persist(state,other);
    check(state.workspace.restoreInConversation(checkpoint.text,logicalId)==="First Synthetic and Second Synthetic","other pending chat blocked this chat");
    reject(()=>state.workspace.restoreInConversation("plain",otherId),"conversation_pending");
    const observed=acceptedSyntheticTransport(state,intent,otherId,"thread_b",otherMarker);
    const confirmation=state.workspace.prepareObservedSend(intent,otherId,observed.surface,observed.siteId,operation());await persist(state,confirmation);
    const report=state.workspace.restoreReportInConversation(`👋 ${otherMarker}`,logicalId);
    check(report.text===null && report.issues[0].kind==="not_issued_in_conversation","wrong chat restored a value");
    const stream=state.workspace.progressiveRestoreInConversation(logicalId);
    try {
      const safe=stream.feed(new TextEncoder().encode("safe 👋 "));
      const failure=reject(()=>stream.feed(new TextEncoder().encode(otherMarker)),"invalid_marker");
      check(failure.complete===false && failure.delivered.bytes===safe.delivered.bytes && failure.delivered.bytes===10,"partial scoped delivery mismatch");
      const transportError=new HostError(failure.code,failure);
      check(transportError.markerIssues?.[0]?.kind==="not_issued_in_conversation","reference error transport lost scoped ranges");
    } finally {stream.close();}
    const before=JSON.stringify((await readRecords()).workspace);
    const blocked=state.workspace.prepareConversationSend([{input:input("Never Sent","Never Sent"),conversation:state.workspace.conversation(logicalId).conversation}],operation());
    let failed=false;try{await persist(state,blocked,{abort:true});}catch(error){failed=error.code==="storage_failure";}
    check(failed && JSON.stringify((await readRecords()).workspace)===before,"first-save failure changed durable state");
    check(state.workspace.recall("Never Sent").findings.length===0,"aborted preparation allocated a saved alias");
    check(transportCalls-startCalls===1,"failed first save called transport");
    return {reopened:true,partialScopedStop:true,firstSaveAbort:true,noAutomaticResend:true,transportCalls:checkpoint.transportCalls+transportCalls-startCalls,counts:state.workspace.counts()};
  } finally {state.workspace.close();}
}
