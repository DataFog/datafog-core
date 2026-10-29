import {writeFileSync} from "node:fs";
import {Workspace} from "@datafog/node";
import {EncryptedWorkspaceStore} from "./node-workspace-host.mjs";
process.once("message",({database,key,scope,input,operationId,phaseFile,killAt})=>{
  const store=new EncryptedWorkspaceStore(database,owner=>owner.tenantId===scope.tenantId && owner.workspaceId===scope.workspaceId ? key : undefined);
  let workspace,bytes;
  try {
    const saved=store.load(scope);
    try {workspace=Workspace.openBundle(saved.bundle);} finally {saved.bundle.fill(0);}
    const plan=workspace.prepareReview([input],operationId),heads=plan.heads();
    bytes=plan.exportCandidateBundle();
    store.saveIfHeadMatches(scope,heads.expectedHead,heads.candidateHead,bytes,phase=>{
      if(phase===killAt) {
        writeFileSync(phaseFile,phase,{mode:0o600,flush:true});
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0);
      }
    });
    process.exitCode=2;
  } catch {process.exitCode=1;}
  finally {bytes?.fill(0);workspace?.close();store.close();key.fill(0);process.disconnect();}
});
