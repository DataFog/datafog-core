// A separate two-native-host profile. No provider/production transport interpretation.
import {copyFile,readFile,writeFile} from "node:fs/promises";
import path from "node:path";
import {createHash} from "node:crypto";
import {isDeepStrictEqual} from "node:util";
const sha=value=>createHash("sha256").update(value).digest("hex");
const bad=()=>{throw new Error("native conversation host qualification failed");};
const same=(value,expected)=>{if(!isDeepStrictEqual(value,expected))bad();};
const requiredCases=["observed-success","pending-save-rollback","callback-mismatch","definitely-not-sent","uncertain-transport","confirmation-save-rollback","confirmation-lost-ack","existing-chat-pause","full-parent-conflict","process-pending-race","process-confirmation-race","crash-pending-before","crash-pending-after","crash-confirmation-before","crash-confirmation-after","context-and-tenant-isolation"];
const stateNames=["seed","pendingA","confirmedA","pendingB","confirmedB","notSentA","alternateConfirmation","otherPending","twoConfirmed","followup","recoveredFollowup","cancelledFollowup"];
const trace=(transportCalls,resolutionPreparations)=>({transportCalls,resolutionPreparations});
function fixtureCheck(fixture){
 if(fixture?.schemaVersion!==1||fixture.profile!=="native-conversation-host-v1")bad();
 same(fixture.sdks,["python","node"]);same(fixture.cases?.map(value=>value.id),requiredCases);
}
export function conversationHostOracle(fixture,result){
 fixtureCheck(fixture);if(result?.schemaVersion!==1||result.profile!==fixture.oracle.profile||result.cases?.length!==1||result.cases[0].id!==fixture.oracle.cases[0].id)bad();
 const states={},plans={};const commands=fixture.oracle.cases[0].commands,steps=result.cases?.[0]?.steps;
 if(fixture.oracle.profile!=="workspace-conversations-v1"||!fixture.oracle.requiredMethods.includes("plan.sendIntents")||commands.length!==steps?.length)bad();
 commands.forEach((command,index)=>{
  const entry=steps[index];if(entry.ok!==true||entry.call!==command.call)bad();
  if(["createWithOperation","openBundle","activateSaved"].includes(command.call))states[command.workspace]={...entry.value};
  else if(["counts","operations","historyCoverage"].includes(command.call))states[command.workspace][command.call]=entry.value;
  else if(["prepareConversationSend","prepareObservedSend","prepareDefinitelyNotSent","prepareRecoveryWithOperation"].includes(command.call))plans[command.plan]=entry.value;
 });
 same(Object.keys(states),stateNames);
 if(!stateNames.every(name=>states[name]?.bundleHex&&states[name].head&&states[name].counts&&states[name].operations&&states[name].historyCoverage))bad();
 if(!["sendA","sendB","otherSend","followup"].every(name=>plans[name]?.sendIntents?.length===1&&typeof plans[name]?.previews?.[0]?.text==="string"))bad();
 return {states,plans};
}
function coreError(value,code){
 if(value?.type!=="core"||value.code!==code||value.message!==code||!Array.isArray(value.markerIssues)||!Array.isArray(value.reviewConflicts)||!Object.hasOwn(value,"inputIndex")||!Object.hasOwn(value,"findingIndex"))bad();
}
function acceptedError(value,fixture,sdk,category){if(value?.type!=="host"||value.message!==value.code||!fixture.hostErrors[category][sdk].includes(value.code))bad();}
export function validateConversationHostOutput(value,fixture,oracle,sdk){
 fixtureCheck(fixture);if(!fixture.sdks.includes(sdk)||value?.schemaVersion!==1||value.profile!==fixture.profile||value.sdk!==sdk||value.cases?.length!==fixture.cases.length)bad();
 const state=name=>oracle.states[name],plan=name=>oracle.plans[name];
 const saved=(receipt,parent,candidate,{error,expected=state(parent).head}={})=>{
  same(receipt?.expectedHead,expected);same(receipt.candidateHead,state(candidate).head);same(receipt.candidateBundleHex,state(candidate).bundleHex);
  if(receipt.saveCalls!==1)bad();
  if(error){if(receipt.ok!==false||receipt.activated!==false||Object.hasOwn(receipt,"acknowledgment"))bad();acceptedError(receipt.error,fixture,sdk,error);same(receipt.localHead,state(parent).head);}
  else{if(receipt.ok!==true||receipt.activated!==true||!Object.hasOwn(receipt,"acknowledgment")||Object.hasOwn(receipt,"error"))bad();same(receipt.acknowledgment,sdk==="node"?state(candidate).head:null);same(receipt.localHead,state(candidate).head);}
 };
 const observation=(entry,{which="sendA",pending="pendingA",kind="observed_sent",mismatched=false}={})=>{
  const intent=plan(which).sendIntents[0],scope=fixture.hostScopes[sdk];
  const expected={scope,preparationId:intent.preparationId,inputIndex:intent.inputIndex,logicalId:Array(16).fill(10),surface:fixture.surface,payloadSha256:sha(Buffer.from(plan(which).previews[0].text))};
  const witness={scope,logicalId:expected.logicalId,surface:fixture.surface,siteId:fixture.siteA};
  same(entry?.attempt,expected);same(entry.siteVerification,witness);same(entry.durableBeforeCallback,state(pending));
  if(mismatched){if(Object.hasOwn(entry,"received"))bad();}
  else same(entry.received,{...expected,siteId:witness.siteId,kind});
  return {...expected,siteId:witness.siteId,kind};
 };
 for(let index=0;index<fixture.cases.length;index++){
  const definition=fixture.cases[index],actual=value.cases[index];
  if(actual.id!==definition.id||actual.steps?.length!==definition.steps.length)bad();
  actual.steps.forEach((entry,index)=>{if(entry.call!==definition.steps[index]||!Object.hasOwn(entry,"value"))bad();});
  const get=name=>actual.steps.find(entry=>entry.call===name)?.value;
  const initial=actual.id==="existing-chat-pause"?"twoConfirmed":"seed";same(actual.before,state(initial));
  let after="pendingA",expectedTrace=trace(1,0);
  switch(actual.id){
   case "observed-success":saved(get("pending-saved"),"seed","pendingA");observation(get("transport-observed"));saved(get("confirmation-saved"),"pendingA","confirmedA");same(get("reopen-confirmed"),state("confirmedA"));after="confirmedA";expectedTrace=trace(1,1);break;
   case "pending-save-rollback":saved(get("save-failed"),"seed","pendingA",{error:"rollback"});same(get("no-dispatch"),{transportEvents:[],resolutionCalls:0});same(get("reopen-original"),state("seed"));after="seed";expectedTrace=trace(0,0);break;
   case "callback-mismatch":{
    saved(get("pending-saved"),"seed","pendingA");const entry=get("callbacks-received"),original=observation(entry,{mismatched:true});
    same(entry.callbacks,[{...original,preparationId:Array(16).fill(99)},{...original,logicalId:Array(16).fill(11)},{...original,siteId:"wrong_site"}]);
    if(entry.transportCalls!==1||entry.resolutionCalls!==0)bad();same(get("no-confirmation"),{resolutionCalls:0});same(get("reopen-pending"),state("pendingA"));break;
   }
   case "definitely-not-sent":saved(get("pending-saved"),"seed","pendingA");observation(get("transport-not-sent"),{kind:"definitely_not_sent"});saved(get("resolution-saved"),"pendingA","notSentA");same(get("reopen-not-sent"),state("notSentA"));after="notSentA";expectedTrace=trace(1,1);break;
   case "uncertain-transport":saved(get("pending-saved"),"seed","pendingA");observation(get("transport-uncertain"),{kind:"uncertain"});same(get("reopen-pending"),state("pendingA"));break;
   case "confirmation-save-rollback":{
    saved(get("pending-saved"),"seed","pendingA");const observed=observation(get("transport-observed"));saved(get("confirmation-failed"),"pendingA","confirmedA",{error:"rollback"});same(get("reopen-pending"),state("pendingA"));
    const retry=get("retry-metadata-only");saved(retry?.save,"pendingA","confirmedA");if(retry.transportCalls!==1)bad();same(retry.received,observed);same(get("reopen-confirmed"),state("confirmedA"));after="confirmedA";expectedTrace=trace(1,2);break;
   }
   case "confirmation-lost-ack":saved(get("pending-saved"),"seed","pendingA");observation(get("transport-observed"));saved(get("confirmation-uncertain"),"pendingA","confirmedA",{error:"lostAck"});same(get("reopen-confirmed"),state("confirmedA"));coreError(get("replay-rejected"),"invalid_send_intent");after="confirmedA";expectedTrace=trace(1,1);break;
   case "existing-chat-pause":{
    same(get("two-confirmed"),state("twoConfirmed"));saved(get("followup-saved"),"twoConfirmed","followup");
    for(const [name,expected] of [["reopen-paused","followup"],["still-paused","recoveredFollowup"]]){
     const entry=get(name);same(entry?.snapshot,state(expected));coreError(entry.plain,"conversation_pending");coreError(entry.old,"conversation_pending");same(entry.other,fixture.inputs.b.input.text);
    }
    saved(get("recovery-saved"),"followup","recoveredFollowup");const stopped=get("not-sent-saved");observation(stopped?.observation,{which:"followup",pending:"recoveredFollowup",kind:"definitely_not_sent"});saved(stopped.save,"recoveredFollowup","cancelledFollowup");
    const resumed=get("reopen-resumed");same(resumed?.snapshot,state("cancelledFollowup"));same(resumed.old,fixture.inputs.a.input.text);coreError(resumed.otherNotIssued,"invalid_marker");if(resumed.otherNotIssued.markerIssues[0]?.kind!=="not_issued_in_conversation")bad();after="cancelledFollowup";expectedTrace=trace(1,1);break;
   }
   case "full-parent-conflict":{
    saved(get("pending-saved"),"seed","pendingA");observation(get("transport-observed"));const parent=state("pendingA").head;
    saved(get("wrong-revision"),"pendingA","confirmedA",{error:"conflict",expected:{...parent,revision:String(BigInt(parent.revision)+1n)}});saved(get("wrong-commit-id"),"pendingA","confirmedA",{error:"conflict",expected:{...parent,commitId:Array(16).fill(99)}});
    const unchanged=get("unchanged-pending");same(unchanged?.snapshot,state("pendingA"));if(!/^[a-f0-9]{64}$/.test(unchanged.rowBeforeSha256??"")||unchanged.rowBeforeSha256!==unchanged.rowAfterSha256)bad();expectedTrace=trace(1,1);break;
   }
   case "process-pending-race":
   case "process-confirmation-race":{
    const confirmation=actual.id==="process-confirmation-race",ready=get("two-processes-ready"),outcomes=get("race");
    if(!Array.isArray(ready)||ready.length!==2||!Array.isArray(outcomes)||outcomes.length!==2)bad();
    if(confirmation){saved(get("pending-saved"),"seed","pendingA");observation(get("transport-observed"));}
    const winners=[];
    for(let index=0;index<2;index++){
     const parent=confirmation?"pendingA":"seed",pending=confirmation?(index===0?"confirmedA":"alternateConfirmation"):(index===0?"pendingA":"pendingB"),confirmed=index===0?"confirmedA":"confirmedB";
     same(ready[index],{ready:true,parent:state(parent).head,candidateHead:state(pending).head,candidateBundleHex:state(pending).bundleHex});
     const result=outcomes[index];same(result.processExit,{code:0,signal:null});
     if(result.save?.ok===true){saved(result.save,parent,pending);winners.push(confirmation?pending:confirmed);}
     else saved(result.save,parent,pending,{error:"conflict"});
     if(confirmation){same(result.transportEvents,[]);same(result.resolutionEvidence,get("transport-observed"));same(result.trace,trace(0,1));}
     else if(result.save.ok){if(result.transportEvents?.length!==1)bad();observation(result.transportEvents[0],{which:index===0?"sendA":"sendB",pending});saved(result.confirmation,pending,confirmed);same(result.trace,trace(1,1));}
     else{same(result.transportEvents,[]);same(result.confirmation,null);same(result.trace,trace(0,0));}
    }
    if(winners.length!==1)bad();after=winners[0];same(get("reopen-exact-winner"),state(after));expectedTrace=confirmation?trace(1,0):trace(0,0);break;
   }
   case "crash-pending-before":
   case "crash-pending-after":
   case "crash-confirmation-before":
   case "crash-confirmation-after":{
    const confirming=actual.id.startsWith("crash-confirmation-"),committed=actual.id.endsWith("after"),entry=get("child-at-boundary");
    const parent=confirming?"pendingA":"seed",candidate=confirming?"confirmedA":"pendingA";
    if(entry?.stage!==(confirming?"confirmation":"pending")||entry.boundary!==(committed?"after_commit":"before_commit"))bad();same(entry.expectedHead,state(parent).head);same(entry.candidateHead,state(candidate).head);same(entry.candidateBundleHex,state(candidate).bundleHex);
    same(entry.trace,confirming?trace(1,1):trace(0,0));if(confirming){if(entry.events?.length!==1)bad();observation(entry.events[0]);}else same(entry.events,[]);
    same(get("killed"),{code:sdk==="python"?-9:null,signal:"SIGKILL"});after=committed?candidate:parent;same(get(definition.steps.at(-1)),state(after));expectedTrace=trace(0,0);break;
   }
   case "context-and-tenant-isolation":{
    saved(get("pending-saved"),"seed","pendingA");const entry=get("foreign-callbacks"),original=observation(entry,{mismatched:true}),scope=fixture.hostScopes[sdk];
    const tenant=sdk==="node"?{...scope,tenantId:"synthetic_foreign"}:{...scope,tenant:Array(16).fill(99)};
    const workspace=sdk==="node"?{...scope,workspaceId:"synthetic_foreign"}:{...scope,workspace:Array(16).fill(99)};
    same(entry.callbacks,[{...original,scope:tenant},{...original,scope:workspace}]);if(entry.transportCalls!==1||entry.resolutionCalls!==0)bad();
    const foreign=get("foreign-store-unchanged");if(!/^[a-f0-9]{64}$/.test(foreign?.before??"")||foreign.before!==foreign.after)bad();same(foreign.snapshot,state("seed"));coreError(get("scoped-unknown"),"conversation_unknown");same(get("unchanged-pending"),state("pendingA"));break;
   }
   default:bad();
  }
  same(actual.after,state(after));same(actual.trace,expectedTrace);
 }
 return value;
}
export async function runNativeConversationHostProfile({root,temporary,python,output,command,validateWorkspaceOutput,existingSuites}){
 const fixtureFile=path.join(root,"fixtures/workspace/native-conversation-host-v1.json"),bytes=await readFile(fixtureFile),fixture=JSON.parse(bytes);fixtureCheck(fixture);
 const oracleFixtureFile=path.join(output,"native-conversation-host-oracle-fixture.json");await writeFile(oracleFixtureFile,JSON.stringify(fixture.oracle,null,2));
 const raw=validateWorkspaceOutput(JSON.parse(command("cargo",["run","--quiet","-p","datafog-core","--example","workspace_contract","--",oracleFixtureFile])),fixture.oracle),oracle=conversationHostOracle(fixture,raw);
 const oracleFile=path.join(output,"native-conversation-host-oracle.json");await writeFile(oracleFile,JSON.stringify(oracle,null,2));await copyFile(fixtureFile,path.join(output,"native-conversation-host-fixture.json"));await writeFile(path.join(output,"native-conversation-host-core-reference.json"),JSON.stringify(raw,null,2));
 if(existingSuites?.python?.passed!==true||existingSuites.python.tests<27||existingSuites.node?.passed!==true||existingSuites.node.checks<17)bad();
 const results={python:JSON.parse(command(python,[path.join(root,"scripts/native-conversation-host-python.py"),fixtureFile,oracleFile]))};
 for(const file of ["native-conversation-host-node.mjs","node-workspace-host.mjs"])await copyFile(path.join(root,"scripts",file),path.join(temporary,file));
 results.node=JSON.parse(command(process.execPath,["native-conversation-host-node.mjs",fixtureFile,oracleFile],temporary));
 for(const sdk of fixture.sdks){await writeFile(path.join(output,`${sdk}-native-conversation-host.json`),JSON.stringify(results[sdk],null,2));validateConversationHostOutput(results[sdk],fixture,oracle,sdk);}
 command(process.execPath,[path.join(root,"scripts/test-native-conversation-host-protocol.mjs"),fixtureFile,oracleFile,path.join(output,"python-native-conversation-host.json"),path.join(output,"node-native-conversation-host.json")]);
 return {profile:fixture.profile,sdks:fixture.sdks,oracleSdk:"rust",cases:fixture.cases.length,steps:fixture.cases.reduce((sum,value)=>sum+value.steps.length,0),fixtureSha256:sha(bytes),existingSuites,scope:"actual native Python/Node encrypted stores only; controlled synthetic callbacks and site verification; no real provider or production host security approval"};
}
