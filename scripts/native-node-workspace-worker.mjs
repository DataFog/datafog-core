import {parentPort} from "node:worker_threads";
import {Workspace} from "@datafog/node";
parentPort.once("message",({bundle,text})=>{
  const workspace=Workspace.openBundle(bundle);
  const result={text:workspace.restore(text),head:workspace.head()};
  workspace.close();
  try {workspace.head();} catch(error) {result.closedCode=error.code;}
  parentPort.postMessage(result);
});
