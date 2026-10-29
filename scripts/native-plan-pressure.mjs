import {readFile,writeFile,copyFile} from "node:fs/promises";
import path from "node:path";
import {createHash} from "node:crypto";
import {isDeepStrictEqual} from "node:util";
const bad=()=>{throw new Error("native plan pressure proof is incomplete");};
export function validatePlanPressure(value,config,sdk){
 if(config.schemaVersion!==1 || !isDeepStrictEqual(config.sdks,["python","node"]) || config.workers!==2 || config.handlesPerWorker!==2 || config.plansPerHandle!==32 || !Number.isSafeInteger(config.cycles) || config.cycles<1 || config.profile!=="native-plan-pressure-v1" || value?.schemaVersion!==1 || value.profile!==config.profile || value.sdk!==sdk || !["python","node"].includes(sdk) || !Array.isArray(value.cycles) || value.cycles.length!==config.cycles)bad();
 const validSample=(sample,pid)=>{
  if(!Number.isSafeInteger(sample?.pid)||sample.pid<=0||!Number.isSafeInteger(sample.rss)||sample.rss<=0||sample.pid!==pid)bad();
  const fields=sdk==="node"?["heapUsed","heapTotal","external","arrayBuffers"]:["pythonTracedBytes","pythonTracedPeakBytes"];
  if(fields.some(field=>!Number.isSafeInteger(sample[field])||sample[field]<0))bad();
 };
 const pids=new Set();
 value.cycles.forEach((cycle,index)=>{
  if(cycle.cycle!==index || cycle.observedLivePlansAtBarrier!==config.workers*config.handlesPerWorker*config.plansPerHandle || !Array.isArray(cycle.workers) || cycle.workers.length!==config.workers)bad();
  cycle.workers.forEach(({held,released},worker)=>{
   const namespace=(20+worker).toString(16).padStart(2,"0").repeat(16);
   if(held.phase!=="held" || released.phase!=="released" || held.worker!==worker || released.worker!==worker || held.cycle!==index || released.cycle!==index || held.handles!==config.handlesPerWorker || held.livePlans!==config.handlesPerWorker*config.plansPerHandle || !Array.isArray(held.admissionErrors) || held.admissionErrors.length!==config.handlesPerWorker || held.admissionErrors.some(code=>code!=="resource_limit") || !held.namespaces?.every(value=>value===namespace) || held.namespaces.length!==config.handlesPerWorker || !Array.isArray(held.heads) || held.heads.length!==config.handlesPerWorker || !held.heads.every(value=>isDeepStrictEqual(value,held.heads[0])) || held.heads[0].revision!==String(index))bad();
   const retiredPerCycle=config.handlesPerWorker+config.handlesPerWorker*config.plansPerHandle+config.plansPerHandle;
   if(released.retainedWrappers!==(index+1)*retiredPerCycle || released.discardSlotReused!==true || released.changedHeadSlotsReused!==32 || released.afterCleanupAdmissionCode!=="resource_limit" || released.staleIndependentHandleRejected!==true || released.staleCasCode!=="workspace_conflict")bad();
   if(!isDeepStrictEqual(released.counts,{operations:index+2,preparedInputs:index+1,transformedSpans:0,confirmedHandoffs:0,reportedBlockedAttempts:0}))bad();
   for(const sample of [held.before,held.held,released.after]){
    validSample(sample,held.before.pid);
   }
   if(index===0)pids.add(held.before.pid);
   else if(held.before.pid!==value.cycles[0].workers[worker].held.before.pid)bad();
  });
 });
 if(pids.size!==config.workers || !Array.isArray(value.exits) || value.exits.length!==config.workers || value.exits.some(value=>value.code!==0||value.signal!==null) || !Array.isArray(value.done) || value.done.length!==config.workers)bad();
 value.done.forEach((done,worker)=>{if(done.phase!=="done" || done.worker!==worker || done.retainedWrappers!==value.cycles.at(-1).workers[worker].released.retainedWrappers || done.retainedReferenceListAfterDrop!==0)bad();const pid=value.cycles[0].workers[worker].held.before.pid;validSample(done.after,pid);validSample(done.afterDrop,pid);});
 return value;
}
export async function runNativePlanPressure({root,temporary,python,output,command}){
 const configFile=path.join(root,"fixtures/workspace/native-plan-pressure-v1.json"),bytes=await readFile(configFile),config=JSON.parse(bytes);
 await copyFile(configFile,path.join(output,"plan-pressure-fixture.json"));
 const core=JSON.parse(command("cargo",["run","--quiet","--release","-p","datafog-core","--example","workspace_operation_capacity","--","--plan-pressure"]));
 if(core.retainedPlanStructs!==32 || core.sourceBytes!==config.sourceBytes || core.sourceSha256!==config.sourceSha256 || core.additionalRequestedBytesAfterDiscard!==0 || !Number.isSafeInteger(core.requestedBytesWhilePending) || !Number.isSafeInteger(core.peakAdditionalRequestedBytes) || core.peakAdditionalRequestedBytes<core.requestedBytesWhilePending || core.requestedBytesWhilePending<32*config.sourceBytes)bad();
 await writeFile(path.join(output,"core-plan-allocation.json"),JSON.stringify(core,null,2));
 const results={};
 results.python=JSON.parse(command(python,[path.join(root,"scripts/native-plan-pressure-python.py"),configFile]));
 await writeFile(path.join(output,"python-plan-pressure.json"),JSON.stringify(results.python,null,2));validatePlanPressure(results.python,config,"python");
 await copyFile(path.join(root,"scripts/native-plan-pressure-node.mjs"),path.join(temporary,"native-plan-pressure-node.mjs"));
 results.node=JSON.parse(command(process.execPath,["native-plan-pressure-node.mjs",configFile],temporary));
 await writeFile(path.join(output,"node-plan-pressure.json"),JSON.stringify(results.node,null,2));validatePlanPressure(results.node,config,"node");
 const extended={...config,cycles:80},extendedFile=path.join(output,"node-plan-pressure-extended-fixture.json");await writeFile(extendedFile,JSON.stringify(extended,null,2));
 const extendedNode=JSON.parse(command(process.execPath,["native-plan-pressure-node.mjs",extendedFile],temporary));
 await writeFile(path.join(output,"node-plan-pressure-extended.json"),JSON.stringify(extendedNode,null,2));validatePlanPressure(extendedNode,extended,"node");
 return {pass:true,profile:config.profile,sdks:["python","node"],fixtureWorkersPerSdk:config.workers,fixtureHandlesPerWorker:config.handlesPerWorker,sdkLimitPerHandle:32,cycles:config.cycles,observedSimultaneousPlansPerSdk:config.workers*config.handlesPerWorker*32,sourceBytesPerPlan:config.sourceBytes,fixtureSha256:createHash("sha256").update(bytes).digest("hex"),coreAllocationAfterDiscardBytes:core.additionalRequestedBytesAfterDiscard,nodeMemoryExtension:{warmupCycles:16,additionalCycles:64,totalCycles:80,dropsRetainedReferences:true},
  memoryScope:"Exact shared-Core requested bytes are separate from observed process RSS and JS/Python heaps. Stale wrappers are retained; native registry allocations are not directly instrumented. Worker processes are joined/reaped. No global Core/host admission limit or hardware SLA is selected."};
}
