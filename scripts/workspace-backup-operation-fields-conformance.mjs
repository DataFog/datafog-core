import {fixtureFieldBytes} from "./workspace-backup-field-bytes.mjs";
const canonical=value=>Array.isArray(value)?value.map(canonical):value&&typeof value==="object"?Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])):value;
const equal=(left,right)=>JSON.stringify(canonical(left))===JSON.stringify(canonical(right));
const bytes=hex=>Uint8Array.from(hex.match(/../g),pair=>Number.parseInt(pair,16));
const check=(value,label)=>{if(!value)throw new Error(label);};

// Caller initializes browser WASM; native Node has no init requirement. No method skips.
export function verifyWorkspaceBackupOperationFields(api,fixture) {
  check(fixture.schemaVersion===1&&fixture.syntheticOnly===true,"operation backup fixture schema");
  for(const name of ["current","legacyUpgraded"]) {
    const gold=fixture[name],original=bytes(gold.bundleHex),workspace=api.Workspace.openBundle(original);
    let candidate,recovered;
    try {
      check(equal([...workspace.exportBundle()],[...original]),"operation fixture byte-exact open");
      check(equal(workspace.head(),gold.head)&&equal(workspace.operations(),gold.operations)&&equal(workspace.counts(),gold.counts),"operation fixture public views");
      check(equal(workspace.historyCoverage(),gold.historyCoverage),"operation fixture coverage");
      for(const alias of gold.aliases)check(workspace.restore(alias.marker)===alias.value,"operation fixture saved alias");
      const first=workspace.operationsPage(0,1000);check(equal(first.records,gold.operations)&&first.nextOffset===null,"operation fixture complete page");
      const abandoned=workspace.prepareRecovery(new Uint8Array(16).fill(111));abandoned.exportCandidateBundle().fill(0);abandoned.discard();
      check(equal([...workspace.exportBundle()],[...original]),"discard recovery changed operation history");
      const plan=workspace.prepareRecovery(new Uint8Array(16).fill(111)),heads=plan.heads();candidate=plan.exportCandidateBundle();
      const pending=api.Workspace.openBundle(candidate);
      try{check(equal(pending.operations().slice(0,gold.operations.length),gold.operations),"staged recovery lost history");}
      finally{pending.close();}
      check(equal(workspace.counts(),gold.counts),"staged recovery activated counts");
      let failure;try{workspace.activateSaved(plan,heads.expectedHead,heads.expectedHead);}catch(error){failure=error;}
      check(failure?.code==="workspace_conflict"&&equal(workspace.counts(),gold.counts),"failed activation changed operation counts");
      workspace.activateSaved(plan,heads.expectedHead,heads.candidateHead);recovered=workspace.exportBundle();
      const before=fixtureFieldBytes(original),after=fixtureFieldBytes(recovered);
      for(const key of [0,1,3,4,5,6,7])check(equal([...before.get(key)],[...after.get(key)]),"recovery changed retained field");
      const rows=workspace.operations(),last=rows.at(-1),counts=workspace.counts();
      check(equal(rows.slice(0,-1),gold.operations)&&last.kind==="recovery"&&last.occurredAtMs===null&&equal(last.id,heads.candidateHead.commitId),"recovery prefix or metadata changed");
      check(equal(counts,{...gold.counts,operations:gold.counts.operations+1}),"recovery changed recorded deltas");
      check(equal(workspace.historyCoverage(),gold.historyCoverage),"recovery invented prior history");
      const retained=gold.operations.find(row=>row.kind==="user_cancelled").id;
      for(const [call,code]of [[()=>workspace.prepare("ordinary",[],Uint8Array.from(retained)),"invalid_operation_id"],[()=>workspace.prepareWithOperation("ordinary",[],{id:Uint8Array.from(retained),occurredAtMs:0}),"operation_reused"]]) {
        let error;try{call();}catch(failure){error=failure;}check(error instanceof api.DataFogError&&error.code===code,"recovery forgot retained operation ID");
      }
      const reopened=api.Workspace.openBundle(recovered);
      try{check(equal(reopened.operations(),rows)&&equal(reopened.counts(),counts),"reopened operation state changed");for(const alias of gold.aliases)check(reopened.restore(alias.marker)===alias.value,"recovered alias changed");}
      finally{reopened.close();}
    } finally{workspace.close();original.fill(0);candidate?.fill(0);recovered?.fill(0);}
  }
  for(const vector of fixture.rejections) {
    const raw=bytes(vector.bundleHex);let error;
    try{api.Workspace.openBundle(raw).close();}catch(failure){error=failure;}finally{raw.fill(0);}
    check(error instanceof api.DataFogError&&error.code===vector.code&&error.message===vector.code,"malformed operation fields accepted");
    check(!error.markerIssues?.length&&!error.reviewConflicts?.length,"snapshot failure exposed source diagnostics");
  }
  return {profile:"workspace-operation-backup-fields-v1",states:2,rejections:fixture.rejections.length};
}
