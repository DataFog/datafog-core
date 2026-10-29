import {parentPort,workerData} from "node:worker_threads";
import {Workspace} from "@datafog/node";
import {EncryptedWorkspaceStore} from "./node-workspace-host.mjs";
const {database,key,scope,input,operationId}=workerData;
const store=new EncryptedWorkspaceStore(database,owner=>owner.tenantId===scope.tenantId && owner.workspaceId===scope.workspaceId ? key : undefined);
let workspace,bytes;
try {
  const saved=store.load(scope);
  try {workspace=Workspace.openBundle(saved.bundle);} finally {saved.bundle.fill(0);}
  const plan=workspace.prepareReview([input],operationId),heads=plan.heads(),text=plan.preview().text;
  bytes=plan.exportCandidateBundle();
  parentPort.postMessage({ready:true,head:workspace.head()});
  await new Promise(resolve=>parentPort.once("message",resolve));
  try {
    const confirmed=store.saveIfHeadMatches(scope,heads.expectedHead,heads.candidateHead,bytes);
    workspace.activateSaved(plan,heads.expectedHead,confirmed);
    parentPort.postMessage({saved:true,activated:true,released:true,text,head:workspace.head()});
  } catch(error) {
    parentPort.postMessage({saved:false,activated:false,released:false,code:error.code,head:workspace.head()});
  }
} finally {bytes?.fill(0);workspace?.close();store.close();key.fill(0);}
