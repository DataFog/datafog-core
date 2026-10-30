import {parentPort,workerData} from 'node:worker_threads';
const {NativeWorkspace}=await import(workerData.module);
try{NativeWorkspace.prototype.head.call(workerData.owner,workerData.tenant);parentPort.postMessage('unexpected-success');}catch(e){parentPort.postMessage(e.code??e.name);}
