import assert from "node:assert/strict";
import {Worker} from "node:worker_threads";
import {verifyWorkspace} from "./workspace-conformance.mjs";

/** Execute the same shared semantic corpus through the installed native SDK. */
export async function verifyNativeWorkspace(api, counters) {
  // The common browser corpus initializes WASM. Native Node needs no initialization;
  // every workspace operation below still calls the installed Rust library.
  const shared = await verifyWorkspace({...api, init: async () => {}}, counters);
  const id = value => new Uint8Array(16).fill(value);
  const throws = (action, code) => assert.throws(action, error =>
    error instanceof api.DataFogError && error.code === code && error.message === code);
  assert.equal(api.NativeWorkspace, undefined, "raw native plan handles are not package exports");
  const namespace=id(51), operation=id(52);
  const workspace=api.Workspace.create(namespace,operation);
  namespace.fill(0);operation.fill(0);
  assert.deepEqual([...workspace.namespace()],[...id(51)],"factory copies host buffers");
  assert.deepEqual(workspace.head().commitId,[...id(52)]);
  const text="👋 Jane";
  const finding={entityType:"PERSON",matchedText:"Jane",range:api.workspaceByteRangeFromUtf16(text,3,7)};
  for (const confidence of [NaN,Infinity,-Infinity]) {
    throws(()=>workspace.prepare(text,[{...finding,confidence}],id(53)),"invalid_finding");
  }
  let accessorCalls=0;
  const accessorFinding={...finding};
  Object.defineProperty(accessorFinding,"matchedText",{enumerable:true,get(){accessorCalls++;return "Jane";}});
  throws(()=>workspace.prepare(text,[accessorFinding],id(53)),"invalid_finding");
  assert.equal(accessorCalls,0,"finding transport must reject getters without executing them");
  const accessorEdits={rules:[],choices:[]};
  Object.defineProperty(accessorEdits,"rules",{enumerable:true,get(){accessorCalls++;return [];}});
  throws(()=>workspace.preparePolicyChanges(accessorEdits,id(53)),"invalid_policy");
  assert.equal(accessorCalls,0,"policy transport must reject getters without executing them");
  const emptyReview={text:"",scannerFindings:[],userSelections:[]};
  for(const request of [emptyReview,{...emptyReview,corrections:[]}]) {
    assert.equal(workspace.review(request).conflicts.length,0);
    workspace.prepareReview([request],id(53)).discard();
  }
  const nullCorrections={...emptyReview,corrections:null};
  throws(()=>workspace.review(nullCorrections),"invalid_finding");
  throws(()=>workspace.prepareReview([nullCorrections],id(53)),"invalid_finding");
  assert.throws(()=>workspace.prepareReview([emptyReview,nullCorrections],id(53)),error=>error.code==="invalid_finding" && error.inputIndex===1);
  const change=workspace.prepare(text,[finding],id(53));
  const heads=change.heads(), encoded=change.exportCandidateBundle();
  const output=change.preview().text;
  const accessorHead={...heads.expectedHead};
  Object.defineProperty(accessorHead,"revision",{enumerable:true,get(){accessorCalls++;return "0";}});
  throws(()=>workspace.activateSaved(change,accessorHead,heads.candidateHead),"workspace_conflict");
  throws(()=>workspace.activateSaved(change,{...heads.expectedHead,revision:0},heads.candidateHead),"workspace_conflict");
  assert.equal(accessorCalls,0,"head transport must reject getters without executing them");
  assert.deepEqual(workspace.head(),heads.expectedHead,"malformed activation cannot mutate committed state");
  const copy=encoded.slice();encoded.fill(0);
  assert.deepEqual(change.exportCandidateBundle(),copy,"snapshot export is a copy, not native backing storage");
  const other=api.Workspace.openBundle(workspace.exportBundle());
  throws(()=>other.activateSaved(change,heads.expectedHead,heads.candidateHead),"workspace_conflict");
  throws(()=>workspace.activateSaved(structuredClone(change),heads.expectedHead,heads.candidateHead),"workspace_conflict");
  throws(()=>api.workspaceByteRangeFromUtf16(text,2**32,2**32),"invalid_finding");
  throws(()=>api.workspaceByteRangeFromUtf16(text,1,2),"invalid_finding");
  const plans=Array.from({length:31},(_,index)=>workspace.prepare("",[],id(80+index)));
  throws(()=>workspace.prepare("",[],id(112)),"resource_limit");
  plans.pop().discard();
  const replacement=workspace.prepare("",[],id(113));
  workspace.activateSaved(change,heads.expectedHead,heads.candidateHead);
  throws(()=>replacement.preview(),"inactive_plan");
  assert.equal(workspace.restore(output),text);

  const worker=new Worker(new URL("./native-node-workspace-worker.mjs",import.meta.url));
  try {
    const result=await new Promise((resolve,reject)=>{
      worker.once("message",resolve);worker.once("error",reject);
      worker.once("exit",code=>{if(code!==0)reject(new Error(`native workspace worker exit ${code}`));});
      worker.postMessage({bundle:workspace.exportBundle(),text:output});
    });
    assert.equal(result.text,text);
    assert.deepEqual(result.head,workspace.head(),"worker reopens independent committed state");
    assert.equal(result.closedCode,"closed_workspace");
    assert.equal(workspace.restore(output),text,"worker close does not close the original handle");
  } finally { await worker.terminate(); }
  workspace.close();other.close();
  throws(()=>change.preview(),"closed_workspace");
  return {shared, nativeOwnership:true, workerReopen:true};
}
