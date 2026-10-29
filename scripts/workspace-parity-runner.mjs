// Shared browser/native Node contract runner. Every operation calls the real public SDK.
const workspaceErrors=new Set(["closed_workspace","inactive_plan","workspace_conflict","invalid_finding","review_conflict","invalid_batch","invalid_marker","invalid_snapshot","invalid_operation_id","invalid_policy","resource_limit","invalid_operation","operation_reused","invalid_conversation","conversation_unknown","conversation_pending","invalid_send_intent","inactive_stream","invalid_encoding"]);
const bad=()=>{throw new Error("workspace parity harness contract failed");};
const unhex=value=>{if(typeof value!=="string" || !/^(?:[a-f0-9]{2})*$/.test(value))bad();return Uint8Array.from(value.match(/../g)??[],item=>Number.parseInt(item,16));};
const textValue=command=>{if(command.textRepeat){const {value,count}=command.textRepeat;if(typeof value!=="string" || !Number.isSafeInteger(count) || count<0 || count>2*1024*1024)bad();return value.repeat(count);}return command.text;};
const hex=bytes=>Array.from(bytes,byte=>byte.toString(16).padStart(2,"0")).join("");
export function canonicalWorkspaceValue(value) {
  if(Array.isArray(value))return value.map(canonicalWorkspaceValue);
  if(value && typeof value==="object") {
    const result=Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonicalWorkspaceValue(value[key])]));
    if(Object.hasOwn(result,"revision") && Object.hasOwn(result,"commitId"))result.revision=String(result.revision);
    if(typeof result.entityType==="string" && typeof result.matchedText==="string" && result.range?.unit==="utf8_bytes") {
      for(const key of ["confidence","detectorName","detectorVersion"])if(!Object.hasOwn(result,key))result[key]=null;
    }
    return result;
  }
  return value;
}
export function runWorkspaceContract(api,fixture) {
  if(fixture.schemaVersion!==1 || !Array.isArray(fixture.cases) || fixture.cases.length===0)bad();
  if(typeof api.Workspace?.create!=="function" || typeof api.Workspace?.openBundle!=="function" || !Array.isArray(fixture.requiredMethods))bad();
  for(const method of fixture.requiredMethods) {
    if(method.startsWith("plan."))continue;
    if(["create","createWithOperation","openBundle"].includes(method) ? typeof api.Workspace[method]!=="function" : typeof api.Workspace.prototype[method]!=="function")bad();
  }
  const conversationProfile=fixture.requiredMethods.includes("prepareConversationSend");
  const output=[];
  for(const scenario of fixture.cases) {
    const workspaces=new Map(),plans=new Map(),planTexts=new Map(),bundles=new Map(),reviews=new Map(),planIntents=new Map(),streams=new Map();
    const steps=[];
    const get=(map,key)=>{if(!map.has(key))bad();return map.get(key);};
    const snapshot=w=>({head:w.head(),namespaceHex:hex(w.namespace()),bundleHex:hex(w.exportBundle()),policy:w.policy()});
    const inspect=p=>({heads:p.heads(),previews:p.previews(),snapshotFormat:p.snapshotFormat(),policyPreview:p.policyPreview(),candidateBundleHex:hex(p.exportCandidateBundle()),...(conversationProfile?{sendIntents:p.sendIntents()}:{})});
    const logical=command=>{if(!Number.isInteger(command.logicalIdByte)||command.logicalIdByte<0||command.logicalIdByte>255)bad();return new Uint8Array(16).fill(command.logicalIdByte);};
    const intent=command=>command.intentFrom?get(planIntents,command.intentFrom.plan)[command.intentFrom.inputIndex]:command.intent;
    const resolvedText=command=>{const text=command.textFrom?get(planTexts,command.textFrom.plan)[command.textFrom.inputIndex]:textValue(command);if(typeof text!=="string")bad();return (command.prefix??"")+text+(command.suffix??"");};
    const sliced=(values,range)=>{if(range===undefined)return values;if(!Array.isArray(range)||range.length!==2||!range.every(Number.isSafeInteger)||range[0]<0||range[1]<range[0]||range[1]>values.length)bad();return values.slice(range[0],range[1]);};
    const resolvedInput=source=>{
      const input=structuredClone(source);
      if(input.textFrom){input.text=resolvedText(input);for(const key of ["textFrom","prefix","suffix"])delete input[key];}
      input.corrections=(input.corrections??[]).map(correction=>{
        if(!correction.candidateFrom)return correction;
        const selector=correction.candidateFrom;
        const matches=get(reviews,selector.review).candidates.filter(candidate=>candidate.finding.entityType===selector.entityType && candidate.byteRange.start===selector.start && candidate.byteRange.end===selector.end);
        if(matches.length!==1)bad();
        return {candidateKey:matches[0].key,replacement:correction.replacement};
      });
      return input;
    };
    const supported=new Set(["prepareConversationSend","prepareObservedSend","prepareDefinitelyNotSent","conversation","conversationForSite","sendIntents","sendIntentStatus","restoreInConversation","restoreReportInConversation","bufferedRestoreInConversation","progressiveRestoreInConversation","streamFeed","streamFeedUtf16","streamFinalize","streamReset","streamClose","createWithOperation","prepareWithOperation","prepareBatchWithOperation","prepareRecoveryWithOperation","prepareOutcome","operations","operationsPage","counts","historyCoverage","create","head","prepare","prepareBatch","prepareReview","preparePolicyChanges","prepareRecovery","inspectPlan","activateSaved","discard","recall","review","restore","restoreReport","restorePlanText","restoreReportPlanText","export","openBundle","close"]);
    if(!Array.isArray(scenario.commands) || scenario.commands.length===0)bad();
    for(const command of scenario.commands) {
      if(!supported.has(command.call))bad();
      let entry;
      try {
        let value;const id=()=>{if(!Number.isInteger(command.operationByte) || command.operationByte<0 || command.operationByte>255)bad();return new Uint8Array(16).fill(command.operationByte);};
        const operation=()=>({id:id(),occurredAtMs:command.occurredAtMs});
        if(["create","createWithOperation"].includes(command.call)) {if(!Number.isInteger(command.namespaceByte) || command.namespaceByte<0 || command.namespaceByte>255)bad();const namespace=new Uint8Array(16).fill(command.namespaceByte);const w=command.call==="create"?api.Workspace.create(namespace,id()):api.Workspace.createWithOperation(namespace,operation());workspaces.set(command.workspace,w);value=snapshot(w);}
        else if(command.call==="openBundle") {const w=api.Workspace.openBundle(command.inlineBundleHex!==undefined?unhex(command.inlineBundleHex):command.fromPlan!==undefined?get(plans,command.fromPlan).exportCandidateBundle():get(bundles,command.bundle));workspaces.set(command.workspace,w);value=snapshot(w);}
        else if(command.call==="conversation")value=get(workspaces,command.workspace).conversation(logical(command));
        else if(command.call==="conversationForSite")value=Array.from(get(workspaces,command.workspace).conversationForSite(command.surface,command.siteId));
        else if(command.call==="sendIntents")value=get(plans,command.plan).sendIntents();
        else if(command.call==="sendIntentStatus")value=get(workspaces,command.workspace).sendIntentStatus(intent(command));
        else if(["restoreInConversation","restoreReportInConversation"].includes(command.call))value=get(workspaces,command.workspace)[command.call](resolvedText(command),logical(command));
        else if(["bufferedRestoreInConversation","progressiveRestoreInConversation"].includes(command.call)) {
          const stream=get(workspaces,command.workspace)[command.call](logical(command),command.encoding??"utf8");
          for(const name of ["feed","feedUtf16","finalize","reset","close"])if(typeof stream[name]!=="function")bad();
          streams.set(command.stream,stream);value=null;
        }
        else if(command.call.startsWith("stream")) {
          const stream=get(streams,command.stream);
          if(command.call==="streamFeed")value=stream.feed(sliced(command.bytes!==undefined?Uint8Array.from(command.bytes):new TextEncoder().encode(resolvedText(command)),command.byteSlice));
          else if(command.call==="streamFeedUtf16") {const text=command.units===undefined?resolvedText(command):"";value=stream.feedUtf16(sliced(command.units!==undefined?Uint16Array.from(command.units):Uint16Array.from({length:text.length},(_,index)=>text.charCodeAt(index)),command.unitSlice));}
          else value=stream[{streamFinalize:"finalize",streamReset:"reset",streamClose:"close"}[command.call]]();
          if(value===undefined)value=null;
        }
        else if(command.call==="head")value=get(workspaces,command.workspace).head();
        else if(command.call==="operations")value=get(workspaces,command.workspace).operations();
        else if(command.call==="counts")value=get(workspaces,command.workspace).counts();
        else if(command.call==="historyCoverage")value=get(workspaces,command.workspace).historyCoverage();
        else if(command.call==="operationsPage")value=get(workspaces,command.workspace).operationsPage(command.offset,command.limit);
        else if(command.call==="export") {const w=get(workspaces,command.workspace);bundles.set(command.bundle,w.exportBundle());value=snapshot(w);}
        else if(command.call==="close"){get(workspaces,command.workspace).close();value=null;}
        else if(command.call==="inspectPlan")value=inspect(get(plans,command.plan));
        else if(command.call==="discard"){get(plans,command.plan).discard();value=null;}
        else if(command.call==="activateSaved"){const w=get(workspaces,command.workspace),p=get(plans,command.plan),h=p.heads();w.activateSaved(p,h.expectedHead,command.savedHeadMode==="parent"?h.expectedHead:h.candidateHead);value=snapshot(w);}
        else if(command.call==="recall")value=get(workspaces,command.workspace).recall(textValue(command));
        else if(command.call==="review"){value=get(workspaces,command.workspace).review(resolvedInput(command.input));reviews.set(command.review,value);}
        else if(["restore","restoreReport","restorePlanText","restoreReportPlanText"].includes(command.call)) {
          let text=command.call.endsWith("PlanText")?get(planTexts,command.plan)[command.inputIndex]:textValue(command);
          if(typeof text!=="string" || (command.suffix!==undefined && typeof command.suffix!=="string"))bad();
          text+=command.suffix??"";
          const w=get(workspaces,command.workspace);value=command.call.startsWith("restoreReport")?w.restoreReport(text):w.restore(text);
        } else {
          const w=get(workspaces,command.workspace);let p;
          if(command.call==="prepareConversationSend")p=w.prepareConversationSend(command.inputs.map(value=>({input:resolvedInput(value.input),conversation:structuredClone(value.conversation)})),operation());
          else if(command.call==="prepareObservedSend")p=w.prepareObservedSend(intent(command),logical(command),command.surface,command.siteId,operation());
          else if(command.call==="prepareDefinitelyNotSent")p=w.prepareDefinitelyNotSent(intent(command),logical(command),operation());
          else if(command.call==="prepare")p=w.prepare(textValue(command),command.findings,id());
          else if(command.call==="prepareWithOperation")p=w.prepareWithOperation(textValue(command),command.findings,operation());
          else if(command.call==="prepareBatchWithOperation")p=w.prepareBatchWithOperation(command.inputs,operation());
          else if(command.call==="prepareRecoveryWithOperation")p=w.prepareRecoveryWithOperation(operation());
          else if(command.call==="prepareOutcome")p=w.prepareOutcome(command.outcome,operation());
          else if(command.call==="prepareBatch")p=w.prepareBatch(command.inputs,id());
          else if(command.call==="prepareReview")p=w.prepareReview(command.inputs.map(resolvedInput),id());
          else if(command.call==="preparePolicyChanges")p=w.preparePolicyChanges(command.edits,id());
          else if(command.call==="prepareRecovery")p=w.prepareRecovery(id());
          else bad();
          value=inspect(p);plans.set(command.plan,p);planTexts.set(command.plan,value.previews.map(preview=>preview.text));if(conversationProfile)planIntents.set(command.plan,value.sendIntents);
        }
        if(command.expectCode)bad();entry={call:command.call,ok:true,value:canonicalWorkspaceValue(value)};
      } catch(error) {
        if(!workspaceErrors.has(error?.code) || error.code!==command.expectCode || error.message!==error.code)bad();
        if(conversationProfile && "text" in error)bad();
        entry={call:command.call,ok:false,error:canonicalWorkspaceValue({code:error.code,inputIndex:error.inputIndex??null,findingIndex:error.findingIndex??null,markerIssues:error.markerIssues??[],reviewConflicts:error.reviewConflicts??[],...(conversationProfile?{delivered:error.delivered??null,complete:error.complete??null}:{})})};
      }
      steps.push(entry);
    }
    for(const stream of streams.values())stream.close();
    for(const w of workspaces.values())w.close();
    output.push({id:scenario.id,steps});
  }
  return canonicalWorkspaceValue({schemaVersion:1,profile:fixture.profile,cases:output});
}
