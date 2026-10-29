import {parentPort} from "node:worker_threads";
import {Workspace} from "@datafog/node";
parentPort.once("message",({bundle,text})=>{
  const workspace=Workspace.openBundle(bundle);
  const result={text:workspace.restore(text),head:workspace.head()};
  const stream=workspace.progressiveRestore();
  result.streamed=stream.feed(new TextEncoder().encode(text)).text+stream.finalize().text;
  stream.close();workspace.close();
  try {workspace.head();} catch(error) {result.closedCode=error.code;}
  parentPort.postMessage(result);
});
