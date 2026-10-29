// Actual installed native Node + existing encrypted SQLite host. Synthetic fixtures only.
import assert from "node:assert/strict";
import {randomBytes,createHash} from "node:crypto";
import {readFileSync,writeFileSync} from "node:fs";
import {mkdtemp,readFile,rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import path from "node:path";
import {fork} from "node:child_process";
import {setTimeout as pause} from "node:timers/promises";
import {DatabaseSync} from "node:sqlite";
import {Workspace} from "@datafog/node";
import {EncryptedWorkspaceStore,HostFailure} from "./node-workspace-host.mjs";
import {verifyNodeWorkspaceHost} from "./node-workspace-host-conformance.mjs";
const id=n=>new Uint8Array(16).fill(n),hex=value=>Buffer.from(value).toString("hex");
const sha=value=>createHash("sha256").update(value).digest("hex");
function snapshot(workspace){return {head:workspace.head(),namespaceHex:hex(workspace.namespace()),bundleHex:hex(workspace.exportBundle()),policy:workspace.policy(),counts:workspace.counts(),operations:workspace.operations(),historyCoverage:workspace.historyCoverage()};}
function open(store,scope){const saved=store.load(scope);try{return Workspace.openBundle(saved.bundle);}finally{saved.bundle.fill(0);}}
function state(store,scope){const workspace=open(store,scope);try{return snapshot(workspace);}finally{workspace.close();}}
function staged(workspace,config){return config.noop?workspace.prepare("ordinary",[],id(config.operationByte)):workspace.prepareWithOperation(config.definition.text,config.definition.findings,{id:id(config.definition.operationByte),occurredAtMs:config.definition.occurredAtMs});}
function row(database,scope){const db=new DatabaseSync(database);try{return db.prepare("SELECT * FROM workspaces WHERE tenant=? AND workspace=?").get(scope.tenantId,scope.workspaceId);}finally{db.close();}}
function rowDigest(database,scope){return sha(JSON.stringify(row(database,scope),(_,value)=>typeof value==="bigint"?String(value):value));}
function errorValue(error){assert(error instanceof HostFailure);assert.equal(error.message,error.code);return {code:error.code,message:error.message};}
function child(config){
 const key=Buffer.from(config.keyHex,"hex"),store=new EncryptedWorkspaceStore(config.database,()=>key);let workspace,plan,bytes;
 try {
  workspace=open(store,config.scope);assert.equal(snapshot(workspace).bundleHex,config.seedHex);
  plan=staged(workspace,config);const heads=plan.heads();bytes=plan.exportCandidateBundle();assert.equal(hex(bytes),config.candidateHex);
  const save=()=>{
   let activated=false;
   try {
    const acknowledgment=store.saveIfHeadMatches(config.scope,heads.expectedHead,heads.candidateHead,bytes,phase=>{
     if(phase===config.crashAt){writeFileSync(config.phaseFile,phase);Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0);}
    });
    workspace.activateSaved(plan,heads.expectedHead,heads.candidateHead);activated=true;
    return {ok:true,operationId:Array.from(id(config.noop?config.operationByte:config.definition.operationByte)),candidateBundleHex:config.candidateHex,head:workspace.head(),acknowledgment,activated,saveCalls:1};
   } catch(error){return {ok:false,operationId:Array.from(id(config.noop?config.operationByte:config.definition.operationByte)),candidateBundleHex:config.candidateHex,error:errorValue(error),activated,saveCalls:1};}
   finally{bytes.fill(0);workspace.close();store.close();key.fill(0);}
  };
  if(config.crashAt){save();process.exitCode=1;return;}
  process.send({ready:true,parent:heads.expectedHead});process.once("message",message=>{assert.equal(message,"save");process.send(save());});
 } catch{bytes?.fill(0);workspace?.close();store.close();key.fill(0);process.exitCode=1;}
}
function childMessage(child){return new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(new Error("native server child timeout")),15000);child.once("message",value=>{clearTimeout(timer);resolve(value);});child.once("error",error=>{clearTimeout(timer);reject(error);});child.once("exit",()=>{clearTimeout(timer);reject(new Error("native server child exited before result"));});});}
async function race(configs,children){
 const pending=configs.map(config=>{const process=fork(new URL(import.meta.url),["--child"],{stdio:["ignore","ignore","ignore","ipc"],serialization:"advanced"});children.push(process);const ready=childMessage(process);process.send(config);return {process,ready};});
 const ready=await Promise.all(pending.map(item=>item.ready));assert(ready.every(value=>value.ready));
 const results=pending.map(item=>{const result=childMessage(item.process);item.process.send("save");return result;});
 return {ready,outcomes:await Promise.all(results)};
}
async function crash(config,children){
 const process=fork(new URL(import.meta.url),["--child"],{stdio:["ignore","ignore","ignore","ipc"],serialization:"advanced"});children.push(process);
 const exited=new Promise(resolve=>process.once("exit",(code,signal)=>resolve({code,signal})));
 process.send(config);let reached=false;
 for(let attempt=0;attempt<1500;attempt++){try{reached=await readFile(config.phaseFile,"utf8")===config.crashAt;}catch(error){if(error.code!=="ENOENT")throw error;}if(reached)break;await pause(10);}
 assert(reached,"real child reached persistence boundary");process.kill("SIGKILL");return {boundary:config.crashAt,processExit:await exited};
}
async function main(){
 const fixture=JSON.parse(readFileSync(process.argv[2],"utf8")),oracle=JSON.parse(readFileSync(process.argv[3],"utf8"));
 const directory=await mkdtemp(path.join(tmpdir(),"datafog-native-server-parity-")),children=[];
 const cases=[];let existingHostSuite;
 try{
  for(const scenario of fixture.cases){
   const database=path.join(directory,scenario.id+".sqlite"),key=randomBytes(32),scope={tenantId:"tenant_11",workspaceId:"workspace_1"};
   const store=new EncryptedWorkspaceStore(database,owner=>owner.tenantId===scope.tenantId&&owner.workspaceId===scope.workspaceId?key:undefined);
   const seed=Workspace.createWithOperation(id(fixture.namespaceByte),{id:id(fixture.bootstrapOperationByte),occurredAtMs:0});
   const seedBytes=seed.exportBundle();assert.equal(hex(seedBytes),oracle.seed.bundleHex);store.initialize(scope,seedBytes);seedBytes.fill(0);seed.close();
   const result={id:scenario.id,before:state(store,scope),steps:[]};
   const record=(call,value)=>result.steps.push({call,value});
   let workspace,plan,candidate;
   try{
    const definition=fixture.candidateDefinitions.a;workspace=open(store,scope);plan=staged(workspace,{definition});candidate=plan.exportCandidateBundle();const heads=plan.heads();assert.equal(hex(candidate),oracle.a.bundleHex);
    const beforeDigest=rowDigest(database,scope);
    const attempt=(parent=heads.expectedHead,fault)=>{try{const acknowledgment=store.saveIfHeadMatches(scope,parent,heads.candidateHead,candidate,fault);return {ok:true,acknowledgment};}catch(error){return {ok:false,error:errorValue(error)};}};
    if(scenario.id==="acknowledged-save"){
     record("open",snapshot(workspace));record("save",attempt());workspace.activateSaved(plan,heads.expectedHead,heads.candidateHead);record("activate",snapshot(workspace));record("reopen",state(store,scope));
    }else if(scenario.id==="full-parent-conflict"){
     record("wrong-revision",attempt({...heads.expectedHead,revision:String(BigInt(heads.expectedHead.revision)+1n)}));record("wrong-commit-id",attempt({...heads.expectedHead,commitId:Array.from(id(99))}));record("unchanged",{rowBeforeSha256:beforeDigest,rowAfterSha256:rowDigest(database,scope),local:snapshot(workspace)});
    }else if(scenario.id==="abort-rollback"){
     record("save-aborted",attempt(heads.expectedHead,phase=>{if(phase==="before_commit")throw new HostFailure("storage_failure");}));record("unchanged",{rowBeforeSha256:beforeDigest,rowAfterSha256:rowDigest(database,scope),local:snapshot(workspace)});
    }else if(scenario.id==="lost-acknowledgment"){
     const attempted=attempt(heads.expectedHead,phase=>{if(phase==="after_commit")throw new HostFailure("save_ack_unknown");});record("save-uncertain",{...attempted,saveCalls:1,local:snapshot(workspace)});record("reopen-committed",state(store,scope));
    }else if(scenario.id.includes("-race")){
     const noop=scenario.id==="process-noop-race";
     const configs=["a","b"].map((name,index)=>({database,scope,keyHex:hex(key),seedHex:oracle.seed.bundleHex,definition:fixture.candidateDefinitions[name],candidateHex:oracle[noop?"noop":name].bundleHex,noop,operationByte:5+index}));
     const evidence=await race(configs,children);record("two-processes-ready",evidence.ready);record("race",evidence.outcomes);record(noop?"reopen-same-head":"reopen-winner",state(store,scope));
    }else if(scenario.id.startsWith("process-crash-")){
     const crashAt=scenario.id==="process-crash-before-commit"?"before_commit":"after_commit";
     const evidence=await crash({database,scope,keyHex:hex(key),seedHex:oracle.seed.bundleHex,definition,candidateHex:oracle.a.bundleHex,crashAt,phaseFile:path.join(directory,scenario.id+".phase")},children);
     record("child-at-boundary",{boundary:evidence.boundary});record("killed",evidence.processExit);record(crashAt==="before_commit"?"reopen-original":"reopen-committed",state(store,scope));
    }else if(scenario.id==="authentication-and-isolation"){
     const original=row(database,scope),db=new DatabaseSync(database);
     try{const altered=Buffer.from(original.ciphertext);altered[0]^=1;db.prepare("UPDATE workspaces SET ciphertext=? WHERE tenant=? AND workspace=?").run(altered,scope.tenantId,scope.workspaceId);try{state(store,scope);throw new Error("tamper accepted");}catch(error){record("tampered-ciphertext",errorValue(error));}finally{db.prepare("UPDATE workspaces SET ciphertext=? WHERE tenant=? AND workspace=?").run(original.ciphertext,scope.tenantId,scope.workspaceId);}}finally{db.close();}
     const metadata=new DatabaseSync(database);
     try{metadata.prepare("UPDATE workspaces SET revision=? WHERE tenant=? AND workspace=?").run("1",scope.tenantId,scope.workspaceId);try{state(store,scope);throw new Error("modified head accepted");}catch(error){record("tampered-head-metadata",errorValue(error));}finally{metadata.prepare("UPDATE workspaces SET revision=? WHERE tenant=? AND workspace=?").run(original.revision,scope.tenantId,scope.workspaceId);}}finally{metadata.close();}
     const wrongKey=Buffer.from(key.map(value=>value^255)),wrong=new EncryptedWorkspaceStore(database,()=>wrongKey);try{try{state(wrong,scope);throw new Error("wrong key accepted");}catch(error){record("wrong-key",errorValue(error));}}finally{wrong.close();wrongKey.fill(0);}
     try{state(store,{...scope,tenantId:"tenant_other"});throw new Error("other tenant accepted");}catch(error){record("other-tenant",errorValue(error));}record("restored-row",{rowBeforeSha256:beforeDigest,rowAfterSha256:rowDigest(database,scope)});
    }else throw new Error("unknown host scenario");
    result.after=state(store,scope);cases.push(result);
   }finally{candidate?.fill(0);workspace?.close();store.close();key.fill(0);}
  }
  existingHostSuite=await verifyNodeWorkspaceHost();
  process.stdout.write(JSON.stringify({schemaVersion:1,profile:fixture.profile,sdk:"node",cases,existingHostSuite}));
 }finally{await Promise.all(children.map(child=>child.exitCode!==null||child.signalCode!==null?undefined:new Promise(resolve=>{child.once("exit",resolve);child.kill("SIGKILL");})));await rm(directory,{recursive:true,force:true});}
}
if(process.argv[2]==="--child")process.once("message",child);else await main();
