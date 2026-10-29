import test from "node:test";
import assert from "node:assert/strict";
import {validateWorkspaceOutput} from "./test-workspace-parity.mjs";
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
