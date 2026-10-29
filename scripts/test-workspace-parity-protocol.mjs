import test from "node:test";
import assert from "node:assert/strict";
import {validateWorkspaceOutput,selectWorkspaceWheel,validateStreamingOutput,validateOperationOutput} from "./test-workspace-parity.mjs";
import {runWorkspaceContract} from "./workspace-parity-runner.mjs";
const fixture={schemaVersion:1,profile:"protocol-test",cases:[{id:"strict",commands:[{call:"restore",expectCode:"invalid_marker"}]}]};
const valid={schemaVersion:1,profile:"protocol-test",cases:[{id:"strict",steps:[{call:"restore",ok:false,error:{code:"invalid_marker",inputIndex:null,findingIndex:null,markerIssues:[],reviewConflicts:[]}}]}]};
test("missing runtime methods fail before any workspace operation",()=>{
  let called=false;
  assert.throws(()=>runWorkspaceContract({Workspace:{create(){called=true;},openBundle(){},prototype:{}}},{schemaVersion:1,requiredMethods:["prepareReview"],cases:[{id:"required",commands:[{call:"create"}]}]}));
  assert.equal(called,false);
});
test("malformed, missing and unlike operation results cannot qualify",()=>{
  assert.deepEqual(validateWorkspaceOutput(valid,fixture),valid);
  for(const value of [null,{}, {...valid,cases:[]},{...valid,profile:"another-contract"},{...valid,cases:[{id:"strict",steps:[]}]},{...valid,cases:[{id:"strict",steps:[{call:"restore",ok:true,value:"partial text"}]}]}])assert.throws(()=>validateWorkspaceOutput(value,fixture));
  const missingRanges=structuredClone(valid);delete missingRanges.cases[0].steps[0].error.reviewConflicts;assert.throws(()=>validateWorkspaceOutput(missingRanges,fixture));
});
test("strict restore reports cannot expose a prefix alongside issues",()=>{
  const expected={schemaVersion:1,profile:"protocol-test",cases:[{id:"strict",commands:[{call:"restoreReport"}]}]};
  const partial={schemaVersion:1,profile:"protocol-test",cases:[{id:"strict",steps:[{call:"restoreReport",ok:true,value:{text:"already restored prefix",restoredMarkers:1,issues:[{kind:"damaged"}]}}]}]};
  assert.throws(()=>validateWorkspaceOutput(partial,expected));
  partial.cases[0].steps[0].value.text=null;assert.doesNotThrow(()=>validateWorkspaceOutput(partial,expected));
});

test("retained historical wheels do not replace or ambiguate the authoritative current Python pin",()=>{
  const project='[project]\nname = "datafog-core"\nversion = "0.5.0b2"\n\n[tool.maturin]\nfeatures = []\n';
  const old='datafog_core-0.5.0b1-cp310-abi3-macosx_11_0_arm64.whl';
  const current='datafog_core-0.5.0b2-cp310-abi3-macosx_11_0_arm64.whl';
  assert.deepEqual(selectWorkspaceWheel([old,current,"unrelated-0.5.0b2-py3-none-any.whl"],project),{filename:current,version:"0.5.0b2"});
  for(const names of [[],[old],[current,'datafog_core-0.5.0b2-cp310-abi3-manylinux_2_17_x86_64.whl']]) assert.throws(()=>selectWorkspaceWheel(names,project));
  for(const invalid of ["",'[tool.maturin]\nversion = "0.5.0b2"',project.replace('version = "0.5.0b2"','version = "0.5.0b2"\nversion = "0.5.0b1"')]) assert.throws(()=>selectWorkspaceWheel([current],invalid));
  assert.deepEqual(selectWorkspaceWheel([current],"[project]\nname='datafog-core'\nversion='0.5.0b2'\n"),{filename:current,version:"0.5.0b2"});
});

test("streaming qualification requires both modes, every command and complete delivery/range metadata",()=>{
  const source={schemaVersion:1,profile:"workspace-streaming-v1",cases:[{id:"feed",steps:[{op:"feed",expect:{text:""}}]}]};
  const value={schemaVersion:1,profile:source.profile,cases:[{id:"feed",modes:{buffered:{steps:[{op:"feed",ok:true,value:null}]},progressive:{steps:[{op:"feed",ok:true,value:{text:"",complete:false,delivered:{bytes:0,codepoints:0,utf16:0,restoredMarkers:0}}}]}}}]};
  assert.doesNotThrow(()=>validateStreamingOutput(value,source));
  const missingMode=structuredClone(value);delete missingMode.cases[0].modes.buffered;
  const missingStep=structuredClone(value);missingStep.cases[0].modes.progressive.steps=[];
  const partial=structuredClone(value);partial.cases[0].modes.progressive.steps[0].value.complete=true;
  const missingCounts=structuredClone(value);delete missingCounts.cases[0].modes.progressive.steps[0].value.delivered;
  for(const invalid of [missingMode,missingStep,partial,missingCounts]) assert.throws(()=>validateStreamingOutput(invalid,source));
  const bad={schemaVersion:1,profile:source.profile,cases:[{id:"feed",steps:[{op:"feed",expectError:{code:"invalid_marker"}}]}]};
  const error={code:"invalid_marker",findingIndex:null,inputIndex:null,markerIssues:[{kind:"damaged",byteRange:{start:0,end:8},codepointRange:{start:0,end:8},utf16Range:{start:0,end:8}}],reviewConflicts:[],delivered:{bytes:0,codepoints:0,utf16:0,restoredMarkers:0},complete:false};
  const failed=structuredClone(value);failed.cases[0].modes.progressive.steps[0]={op:"feed",ok:false,error};
  assert.doesNotThrow(()=>validateStreamingOutput(failed,bad));
  for(const mutate of [v=>delete v.delivered,v=>delete v.markerIssues[0].utf16Range,v=>{v.text="partial private text";}]) {
    const invalid=structuredClone(failed);mutate(invalid.cases[0].modes.progressive.steps[0].error);assert.throws(()=>validateStreamingOutput(invalid,bad));
  }
});

const operationMethods=["createWithOperation","prepareWithOperation","prepareBatchWithOperation","prepareRecoveryWithOperation","prepareOutcome","operations","operationsPage","counts","historyCoverage"];
test("operation profile cannot skip missing native methods or its required calls",()=>{
  for(const missing of operationMethods) {
    let called=false;
    class Workspace {static create(){called=true;}static createWithOperation(){called=true;}static openBundle(){}}
    for(const method of operationMethods)if(method!=="createWithOperation")Workspace.prototype[method]=()=>{};
    delete (missing==="createWithOperation"?Workspace:Workspace.prototype)[missing];
    assert.throws(()=>runWorkspaceContract({Workspace},{schemaVersion:1,profile:"workspace-operations-v1",requiredMethods:operationMethods,cases:[{id:"required",commands:[{call:"createWithOperation"}]}]}));
    assert.equal(called,false);
  }
});
test("operation output retains complete counts, unknown times, explicit pages and content-free rows",()=>{
  const row={id:Array(16).fill(2),kind:"create",occurredAtMs:null,preparedInputs:0,transformedSpans:0};
  const counts={operations:1,preparedInputs:0,transformedSpans:0,confirmedHandoffs:0,reportedBlockedAttempts:0};
  const coverage={recordedHistoryOnly:true,preUpgradeHistory:"unknown"};
  const source={schemaVersion:1,profile:"workspace-operations-v1",requiredMethods:operationMethods,cases:[{id:"reads",commands:[{call:"operations",expectValue:[row]},{call:"counts",expectValue:counts},{call:"historyCoverage",expectValue:coverage},{call:"operationsPage",offset:0,limit:1}]}]};
  const value={schemaVersion:1,profile:source.profile,cases:[{id:"reads",steps:[{call:"operations",ok:true,value:[row]},{call:"counts",ok:true,value:counts},{call:"historyCoverage",ok:true,value:coverage},{call:"operationsPage",ok:true,value:{records:[row],total:2,offset:0,nextOffset:1}}]}]};
  assert.doesNotThrow(()=>validateOperationOutput(value,source));
  for(const mutate of [
    v=>{v.cases[0].steps[0].value[0].occurredAtMs=0;},
    v=>{v.cases[0].steps[0].value[0].content="private input";},
    v=>{delete v.cases[0].steps[1].value.reportedBlockedAttempts;},
    v=>{v.cases[0].steps[2].value.preUpgradeHistory="not_applicable";},
    v=>{v.cases[0].steps[3].value.nextOffset=null;},
    v=>{v.cases[0].steps[3].value.records=[];},
    v=>{v.cases[0].steps.pop();},
  ]){const bad=structuredClone(value);mutate(bad);assert.throws(()=>validateOperationOutput(bad,source));}
  assert.throws(()=>validateOperationOutput(value,{...source,requiredMethods:[]}));
});
test("read/discard/failed-save byte preservation is independently asserted",()=>{
  const source={schemaVersion:1,profile:"protocol-test",cases:[{id:"saved",commands:[{call:"export",bundle:"before"},{call:"export",bundle:"after",sameBundleAs:"before"}]}]};
  const result={schemaVersion:1,profile:source.profile,cases:[{id:"saved",steps:[{call:"export",ok:true,value:{bundleHex:"a10001"}},{call:"export",ok:true,value:{bundleHex:"a10001"}}]}]};
  assert.doesNotThrow(()=>validateWorkspaceOutput(result,source));
  result.cases[0].steps[1].value.bundleHex="a10002";assert.throws(()=>validateWorkspaceOutput(result,source));
});
