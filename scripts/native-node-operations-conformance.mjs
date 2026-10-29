import assert from "node:assert/strict";
import {verifyWorkspaceOperations} from "./workspace-conformance.mjs";

/** Shared Core/WASM behavior plus native transport, ownership and lifetime checks. */
export async function verifyNativeOperations(api, legacyFixtures) {
  await verifyWorkspaceOperations({...api,init:async()=>{}});
  const id=value=>new Uint8Array(16).fill(value),context=value=>({id:id(value),occurredAtMs:0});
  for(const fixture of legacyFixtures) {
    const bytes=Uint8Array.from(fixture.bundleHex.match(/../g),value=>Number.parseInt(value,16));
    const legacy=api.Workspace.openBundle(bytes);
    assert.deepEqual(legacy.operations(),[]);assert.equal(legacy.counts().operations,0);
    assert.deepEqual(legacy.historyCoverage(),{recordedHistoryOnly:true,preUpgradeHistory:"unknown"});
    assert.deepEqual(legacy.operationsPage(0,1000),{records:[],total:0,offset:0,nextOffset:null});
    const unchanged=legacy.prepare("",[],id(170));assert.deepEqual(unchanged.exportCandidateBundle(),bytes);unchanged.discard();
    const timed=legacy.prepareWithOperation("",[],context(171));
    const candidate=api.Workspace.openBundle(timed.exportCandidateBundle());
    try {assert.equal(candidate.counts().operations,1);assert.equal(candidate.operations()[0].kind,"prepare");assert.equal(candidate.historyCoverage().preUpgradeHistory,"unknown");}
    finally {candidate.close();timed.discard();}
    assert.deepEqual(legacy.exportBundle(),bytes,"reads, legacy no-op and discarded timed upgrade preserve exact v1/v2 bytes");legacy.close();
  }
  const workspace=api.Workspace.createWithOperation(id(101),context(102));
  const before=workspace.exportBundle();let accessorCalls=0;
  const invalidContext={occurredAtMs:0};Object.defineProperty(invalidContext,"id",{enumerable:true,get(){accessorCalls++;return id(103);}});
  assert.throws(()=>workspace.prepareRecoveryWithOperation(invalidContext),error=>error.code==="invalid_operation");
  const invalidOutcome={issueCount:0};Object.defineProperty(invalidOutcome,"kind",{enumerable:true,get(){accessorCalls++;return "copy_confirmed";}});
  assert.throws(()=>workspace.prepareOutcome(invalidOutcome,context(103)),error=>error.code==="invalid_operation");
  assert.equal(accessorCalls,0);assert.deepEqual(workspace.exportBundle(),before);
  const plans=Array.from({length:32},(_,index)=>workspace.prepareWithOperation("ordinary",[],context(110+index)));
  assert.throws(()=>workspace.prepareOutcome({kind:"user_cancelled",issueCount:0},context(150)),error=>error.code==="resource_limit");
  assert.equal(workspace.counts().operations,1,"pending plans do not count as committed operations");
  plans.pop().discard();
  const replacement=workspace.prepareRecoveryWithOperation(context(151));
  const chosen=plans[0],heads=chosen.heads();workspace.activateSaved(chosen,heads.expectedHead,heads.candidateHead);
  assert.throws(()=>replacement.heads(),error=>error.code==="inactive_plan");
  assert.equal(workspace.counts().operations,2);
  const saved=workspace.exportBundle(),records=workspace.operationsPage(0,1);records.records[0].id.fill(0);
  assert.deepEqual(workspace.exportBundle(),saved,"paged read results are owned and never alter saved history");
  workspace.close();
  for(const call of [()=>workspace.operations(),()=>workspace.operationsPage(0,1),()=>workspace.counts(),()=>workspace.historyCoverage(),()=>workspace.prepareRecoveryWithOperation(context(152)),()=>workspace.prepareOutcome({kind:"copy_confirmed",issueCount:0},context(153))])assert.throws(call,error=>error.code==="closed_workspace");
}
