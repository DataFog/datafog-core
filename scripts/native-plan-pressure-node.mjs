// Actual installed N-API handles; private process ownership is deliberately observable.
import assert from "node:assert/strict";
import {createHash,randomBytes} from "node:crypto";
import {readFileSync} from "node:fs";
import {mkdtemp,rm} from "node:fs/promises";
import path from "node:path";
import {tmpdir} from "node:os";
import {fork} from "node:child_process";
import {setImmediate} from "node:timers/promises";
import {Workspace} from "@datafog/node";
import {EncryptedWorkspaceStore,HostFailure} from "./node-workspace-host.mjs";
const id=n=>new Uint8Array(16).fill(n);
function operation(worker,cycle,handle,phase,slot){const value=new Uint8Array(16);value.set([250,worker,cycle,handle,phase,slot]);return {id:value,occurredAtMs:0};}
function failure(action,code){let observed;assert.throws(action,error=>{observed=error.code;return observed===code&&error.message===code;});return observed;}
function open(store,scope){const saved=store.load(scope);try{return Workspace.openBundle(saved.bundle);}finally{saved.bundle.fill(0);}}
async function sample(){globalThis.gc();await setImmediate();await setImmediate();return {pid:process.pid,...process.memoryUsage()};}
function sourceText(config){let source="";for(let index=0;source.length<config.sourceBytes;index++)source+=createHash("sha256").update(`native-plan-pressure:${index}`).digest("hex");source=source.slice(0,config.sourceBytes);assert.equal(createHash("sha256").update(source).digest("hex"),config.sourceSha256);return source;}
function receive(child){return new Promise((resolve,reject)=>{
 const cleanup=()=>{clearTimeout(timer);child.removeListener("message",message);child.removeListener("error",error);child.removeListener("exit",exit);};
 const message=value=>{cleanup();resolve(value);},error=value=>{cleanup();reject(value);},exit=()=>{cleanup();reject(new Error("native plan pressure child exited early"));};
 const timer=setTimeout(()=>error(new Error("native plan pressure child timeout")),30000);child.once("message",message);child.once("error",error);child.once("exit",exit);
});}
async function worker({config,database,scope,keyHex,worker}){
 assert.equal(typeof globalThis.gc,"function");const key=Buffer.from(keyHex,"hex"),store=new EncryptedWorkspaceStore(database,()=>key),source=sourceText(config),retained=[];
 try{
  for(let cycle=0;cycle<config.cycles;cycle++){
   const handles=Array.from({length:config.handlesPerWorker},()=>open(store,scope));const before=await sample(),admissionErrors=[];
   const plans=handles.map((handle,index)=>{
    const rows=Array.from({length:config.plansPerHandle},(_,slot)=>handle.prepareWithOperation(source,[],operation(worker,cycle,index,0,slot)));
    admissionErrors.push(failure(()=>handle.prepareWithOperation(source,[],operation(worker,cycle,index,3,0)),"resource_limit"));
    const discarded=rows.pop();discarded.discard();failure(()=>discarded.heads(),"inactive_plan");retained.push(discarded);
    rows.push(handle.prepareWithOperation(source,[],operation(worker,cycle,index,1,31)));return rows;
   });
   const held=await sample();process.send({phase:"held",worker,cycle,handles:handles.length,admissionErrors,namespaces:handles.map(handle=>Buffer.from(handle.namespace()).toString("hex")),heads:handles.map(handle=>handle.head()),livePlans:plans.reduce((sum,value)=>sum+value.length,0),before,held});
   const command=await new Promise(resolve=>process.once("message",resolve));assert.equal(command,"release");
   const chosen=plans[0][0],heads=chosen.heads(),bundle=chosen.exportCandidateBundle();
   try{store.saveIfHeadMatches(scope,heads.expectedHead,heads.candidateHead,bundle);handles[0].activateSaved(chosen,heads.expectedHead,heads.candidateHead);}finally{bundle.fill(0);}
   for(const plan of plans[0])failure(()=>plan.heads(),"inactive_plan");
   const recycled=Array.from({length:32},(_,slot)=>handles[0].prepareWithOperation(source,[],operation(worker,cycle,0,2,slot)));
   const afterCleanupAdmissionCode=failure(()=>handles[0].prepareWithOperation(source,[],operation(worker,cycle,0,3,1)),"resource_limit");
   for(const plan of recycled){plan.discard();failure(()=>plan.heads(),"inactive_plan");}retained.push(...recycled);
   // A separately opened handle has its own slots and stale local view. Only CAS
   // detects another handle's saved head; local Core is not a global coordinator.
   const stale=plans[1][0],staleHeads=stale.heads(),staleBytes=stale.exportCandidateBundle();
   let staleCasCode;
   try{staleCasCode=failure(()=>store.saveIfHeadMatches(scope,staleHeads.expectedHead,staleHeads.candidateHead,staleBytes),"workspace_conflict");}finally{staleBytes.fill(0);}
   assert.deepEqual(stale.heads(),staleHeads);
   retained.push(...plans.flat());for(const handle of handles)handle.close();
   for(const plan of plans.flat())failure(()=>plan.heads(),"closed_workspace");
   const saved=open(store,scope);let counts;
   try{counts=saved.counts();assert.equal(counts.operations,cycle+2);assert.equal(counts.preparedInputs,cycle+1);assert.equal(counts.transformedSpans,0);}finally{saved.close();}
   process.send({phase:"released",worker,cycle,retainedWrappers:retained.length,counts,after:await sample(),discardSlotReused:true,changedHeadSlotsReused:32,staleIndependentHandleRejected:true,staleCasCode,afterCleanupAdmissionCode});
   assert.equal(await new Promise(resolve=>process.once("message",resolve)),"next");
  }
  for(const plan of retained)failure(()=>plan.heads(),"closed_workspace");
  const retainedWrappers=retained.length,after=await sample();retained.length=0;
  process.send({phase:"done",worker,retainedWrappers,retainedReferenceListAfterDrop:retained.length,after,afterDrop:await sample()});
  assert.equal(await new Promise(resolve=>process.once("message",resolve)),"finish");
 }finally{store.close();key.fill(0);process.disconnect();}
}
async function main(){
 const config=JSON.parse(readFileSync(process.argv[2],"utf8")),directory=await mkdtemp(path.join(tmpdir(),"datafog-node-plan-pressure-")),keys=[],children=[];
 assert.equal(config.workers,2);assert.equal(config.handlesPerWorker,2);assert.equal(config.plansPerHandle,32);
 const database=path.join(directory,"pressure.sqlite");const scopes=Array.from({length:2},(_,index)=>({tenantId:"synthetic_pressure",workspaceId:`workspace_${20+index}`}));
 const store=new EncryptedWorkspaceStore(database,scope=>keys[scopes.findIndex(value=>value.workspaceId===scope.workspaceId&&value.tenantId===scope.tenantId)]);
 const cycles=[];
 try{
  for(let index=0;index<2;index++){keys.push(randomBytes(32));const workspace=Workspace.createWithOperation(id(20+index),{id:id(2),occurredAtMs:0});try{const bundle=workspace.exportBundle();try{store.initialize(scopes[index],bundle);}finally{bundle.fill(0);}}finally{workspace.close();}}
  for(let index=0;index<2;index++){
   const child=fork(new URL(import.meta.url),["--child"],{execArgv:["--expose-gc"],stdio:["ignore","ignore","ignore","ipc"],serialization:"advanced"});
   const first=receive(child);children.push({child,next:first,exit:new Promise(resolve=>child.once("exit",(code,signal)=>resolve({code,signal})))});child.send({config,database,scope:scopes[index],keyHex:keys[index].toString("hex"),worker:index});
  }
  for(let cycle=0;cycle<config.cycles;cycle++){
   const held=await Promise.all(children.map(value=>value.next));assert(held.every(value=>value.phase==="held"&&value.cycle===cycle&&value.livePlans===64));
   const released=[];
   for(let index=0;index<children.length;index++){
    const value=children[index],next=receive(value.child);value.child.send("release");const result=await next;assert.equal(result.phase,"released");assert.equal(result.cycle,cycle);released.push(result);value.next=receive(value.child);value.child.send("next");
   }
   cycles.push({cycle,observedLivePlansAtBarrier:held.reduce((sum,value)=>sum+value.livePlans,0),workers:held.map((value,index)=>({held:value,released:released[index]}))});
  }
  const done=await Promise.all(children.map(value=>value.next));assert(done.every(value=>value.phase==="done"));
  for(const value of children)value.child.send("finish");
  const exits=await Promise.all(children.map(value=>value.exit));assert(exits.every(value=>value.code===0&&value.signal===null));
  process.stdout.write(JSON.stringify({schemaVersion:1,profile:config.profile,sdk:"node",cycles,done,exits,scope:"per-handle admission only; process RSS/JS heap are observations, not exact native allocation counters or a host-wide budget"}));
 }finally{await Promise.all(children.map(({child})=>child.exitCode!==null||child.signalCode!==null?undefined:new Promise(resolve=>{child.once("exit",resolve);child.kill("SIGKILL");})));store.close();for(const key of keys)key.fill(0);await rm(directory,{recursive:true,force:true});}
}
if(process.argv[2]==="--child")process.once("message",value=>worker(value).catch(()=>{process.exitCode=1;process.disconnect();}));else await main();
