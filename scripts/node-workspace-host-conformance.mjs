import assert from "node:assert/strict";
import {mkdtemp,readFile,readdir,rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import path from "node:path";
import crypto,{randomBytes} from "node:crypto";
import {syncBuiltinESMExports} from "node:module";
import {setTimeout} from "node:timers/promises";
import {Worker} from "node:worker_threads";
import {fork} from "node:child_process";
import {DatabaseSync} from "node:sqlite";
import {Workspace} from "@datafog/node";
import {EncryptedWorkspaceStore,HostFailure,runPreparedRequest,restoreStoredResponse,restoreStoredResponseProgressively} from "./node-workspace-host.mjs";

export async function verifyNodeWorkspaceHost() {
  const directory=await mkdtemp(path.join(tmpdir(),"datafog-node-host-"));
  const database=path.join(directory,"workspaces.sqlite"),key=randomBytes(32),otherKey=randomBytes(32);
  const scope={tenantId:"tenant_alpha",workspaceId:"workspace_alpha"};
  const id=value=>new Uint8Array(16).fill(value);
  const input=value=>({text:value,scannerFindings:[],userSelections:[{entityType:"PERSON",matchedText:value,range:{unit:"utf8_bytes",start:0,end:Buffer.byteLength(value)}}]});
  const scopeKey=owner=>`${owner.tenantId}/${owner.workspaceId}`;
  const authorized=new Map();
  const keyFor=owner=>authorized.get(scopeKey(owner));
  const store=new EncryptedWorkspaceStore(database,keyFor);
  const initialize=(owner,namespace)=>{
    authorized.set(scopeKey(owner),owner.tenantId===scope.tenantId?key:otherKey);
    const workspace=Workspace.create(id(namespace),id(namespace+1));
    let bytes;
    try {bytes=workspace.exportBundle();return store.initialize(owner,bytes);}
    finally {bytes?.fill(0);workspace.close();}
  };
  const head=owner=>{const saved=store.load(owner);try{return saved.head;}finally{saved.bundle.fill(0);}};
  const open=owner=>{const saved=store.load(owner);try{return Workspace.openBundle(saved.bundle);}finally{saved.bundle.fill(0);}};
  const rejected=(action,code)=>assert.rejects(action,error=>error.code===code && error.message===code);
  const released=[];
  const run=(owner,requestId,value,operationId,options={})=>runPreparedRequest({store,scope:owner,requestId,input:input(value),operationId:id(operationId),approve:async()=>true,release:async result=>released.push({owner,...result}),...options});
  const workers=[];
  try {
    const initial=initialize(scope,1);
    const value="Synthetic Race Name";
    for(const operation of [21,22]) {
      const worker=new Worker(new URL("./node-workspace-host-worker.mjs",import.meta.url),{workerData:{database,key:new Uint8Array(key),scope,input:input(value),operationId:id(operation)}});
      const ready=new Promise((resolve,reject)=>{worker.once("message",resolve);worker.once("error",reject);});
      workers.push({worker,ready});
    }
    for(const item of workers) assert.deepEqual((await item.ready).head,initial);
    const results=workers.map(({worker})=>new Promise((resolve,reject)=>{worker.once("message",resolve);worker.once("error",reject);worker.postMessage("save");}));
    const outcomes=await Promise.all(results),winners=outcomes.filter(result=>result.saved),losers=outcomes.filter(result=>!result.saved);
    assert.equal(winners.length,1);assert.equal(losers.length,1);
    assert(winners[0].activated && winners[0].released);
    assert(!losers[0].activated && !losers[0].released);
    assert(["workspace_conflict","storage_busy"].includes(losers[0].code));
    assert.deepEqual(losers[0].head,initial);
    const winner=open(scope);
    try {assert.equal(winner.restore(winners[0].text),value);}finally{winner.close();}

    // No-op candidates intentionally keep their head; CAS is not a delivery lock.
    const noOpScope={...scope,workspaceId:"noop_race"},noOpHead=initialize(noOpScope,23);
    const noOpWorkers=[];
    for(const operation of [25,26]) {
      const worker=new Worker(new URL("./node-workspace-host-worker.mjs",import.meta.url),{workerData:{database,key:new Uint8Array(key),scope:noOpScope,input:{text:"ordinary",scannerFindings:[],userSelections:[]},operationId:id(operation)}});
      const ready=new Promise((resolve,reject)=>{worker.once("message",resolve);worker.once("error",reject);});
      const item={worker,ready};workers.push(item);noOpWorkers.push(item);
    }
    for(const item of noOpWorkers)assert.deepEqual((await item.ready).head,noOpHead);
    const noOpOutcomes=await Promise.all(noOpWorkers.map(({worker})=>new Promise((resolve,reject)=>{
      worker.once("message",resolve);worker.once("error",reject);worker.postMessage("save");
    })));
    assert(noOpOutcomes.every(result=>result.saved && result.activated && result.released));
    assert(noOpOutcomes.every(result=>JSON.stringify(result.head)===JSON.stringify(noOpHead)));

    // Equal revisions with different commit IDs must not pass a revision-only CAS.
    const sameRevisionScope={...scope,workspaceId:"same_revision"};initialize(sameRevisionScope,31);
    const local=open(sameRevisionScope),plan=local.prepareReview([input("Synthetic Revision Name")],id(33));
    const heads=plan.heads(),candidate=plan.exportCandidateBundle();
    try {
      assert.throws(()=>store.saveIfHeadMatches(sameRevisionScope,{...heads.expectedHead,commitId:[...id(99)]},heads.candidateHead,candidate),error=>error.code==="workspace_conflict");
      assert.deepEqual(local.head(),heads.expectedHead);
    } finally {candidate.fill(0);local.close();}

    const busy=new DatabaseSync(database,{timeout:250});busy.exec("BEGIN IMMEDIATE");
    const busyStart=performance.now();
    try {await rejected(()=>run(sameRevisionScope,"busy_request","Synthetic Busy Name",34),"storage_busy");}
    finally {busy.exec("ROLLBACK");busy.close();}
    assert(performance.now()-busyStart<3000,"busy storage has a bounded fail path");assert.equal(released.length,0);

    const failedScope={...scope,workspaceId:"failed_save"};const failedHead=initialize(failedScope,41);
    await rejected(()=>run(failedScope,"failed_request","Synthetic Failed Name",43,{fault:phase=>{if(phase==="before_commit")throw new HostFailure("storage_failure");}}),"storage_failure");
    assert.deepEqual(head(failedScope),failedHead);assert.equal(released.length,0);

    await rejected(()=>run(failedScope,"changed_approval","Synthetic Approval Name",44,{approve:async presentation=>{
      presentation.previews[0].text="Synthetic Unapproved Replacement";
      presentation.head.commitId.fill(0);return true;
    }}),"invalid_approval");
    assert.deepEqual(head(failedScope),failedHead);assert.equal(released.length,0);

    const lostScope={...scope,workspaceId:"lost_ack"};initialize(lostScope,51);
    await rejected(()=>run(lostScope,"lost_request","Synthetic Lost Name",53,{fault:phase=>{if(phase==="after_commit")throw new HostFailure("save_ack_unknown");}}),"save_ack_unknown");
    assert.equal(released.length,0);
    let freshApprovals=0;
    await run(lostScope,"fresh_request","Synthetic Lost Name",54,{approve:async()=>{freshApprovals++;return true;}});
    assert.equal(freshApprovals,1);assert.equal(released.length,1);
    const recovered=open(lostScope);try{assert.equal(recovered.restore(released.at(-1).text),"Synthetic Lost Name");}finally{recovered.close();}

    for(const phase of ["before_commit","after_commit"]) {
      const owner={...scope,workspaceId:phase};const oldHead=initialize(owner,phase==="before_commit"?61:71);
      const phaseFile=path.join(directory,phase),child=fork(new URL("./node-workspace-host-crash.mjs",import.meta.url),[],{serialization:"advanced",stdio:["ignore","ignore","ignore","ipc"]});
      const exited=new Promise((resolve,reject)=>{child.once("exit",resolve);child.once("error",reject);});
      try {
        child.send({database,key:new Uint8Array(key),scope:owner,input:input("Synthetic Crash Name"),operationId:id(phase==="before_commit"?63:73),phaseFile,killAt:phase});
        let reached=false;
        for(let attempt=0;attempt<1000;attempt++) {
          try {reached=(await readFile(phaseFile,"utf8"))===phase;} catch(error) {if(error.code!=="ENOENT")throw error;}
          if(reached)break;await setTimeout(10);
        }
        assert(reached,"actual child process reached the persistence fault boundary");
        child.kill("SIGKILL");await exited;
        const reopened=open(owner);
        try {
          assert.equal(reopened.head().revision,phase==="before_commit"?oldHead.revision:String(BigInt(oldHead.revision)+1n));
          assert.equal(reopened.recall("Synthetic Crash Name").findings.length,phase==="before_commit"?0:1);
        } finally {reopened.close();}
      } finally {if(child.exitCode===null && child.signalCode===null){child.kill("SIGKILL");await exited;}}
    }

    const cancelScope={...scope,workspaceId:"cancelled"};const cancelHead=initialize(cancelScope,81);
    const before=new AbortController();before.abort();
    const count=released.length;
    await rejected(()=>run(cancelScope,"cancel_before","Synthetic Cancel Name",83,{signal:before.signal}),"cancelled");
    const after=new AbortController();
    await rejected(()=>run(cancelScope,"cancel_after","Synthetic Cancel Name",84,{signal:after.signal,fault:phase=>{if(phase==="after_commit")after.abort();}}),"cancelled");
    assert.equal(released.length,count);assert.notDeepEqual(head(cancelScope),cancelHead);

    // Once release starts, the host callback must cooperate before its final write.
    const pausedScope={...scope,workspaceId:"paused_release"};initialize(pausedScope,85);
    const pausedSignal=new AbortController();let reachedRelease,resumeRelease;
    const enteredRelease=new Promise(resolve=>{reachedRelease=resolve;});
    const pausedWrite=new Promise(resolve=>{resumeRelease=resolve;});
    const pendingRelease=run(pausedScope,"paused_request","Synthetic Paused Name",87,{signal:pausedSignal.signal,release:async result=>{
      reachedRelease();await pausedWrite;
      if(result.signal?.aborted)throw new HostFailure("cancelled");
      released.push({owner:pausedScope,...result});
    }});
    await enteredRelease;pausedSignal.abort();resumeRelease();
    await rejected(()=>pendingRelease,"cancelled");assert.equal(released.length,count);
    assert.equal(head(pausedScope).revision,"1","cooperative cancellation does not roll back committed mappings");

    // Approval of an older generation cannot survive a competing commit.
    const staleScope={...scope,workspaceId:"stale_approval"};initialize(staleScope,91);
    await rejected(()=>run(staleScope,"stale_request","Synthetic Old Name",93,{approve:async()=>{
      await run(staleScope,"new_request","Synthetic New Name",94);return true;
    }}),"workspace_conflict");
    assert.equal(released.at(-1).requestId,"new_request");

    const secondScope={tenantId:"tenant_beta",workspaceId:"workspace_beta"};initialize(secondScope,101);
    await Promise.all([run(scope,"request_alpha","Synthetic Alpha Name",111),run(secondScope,"request_beta","Synthetic Beta Name",112)]);
    for(const [owner,requestId,expected] of [[scope,"request_alpha","Synthetic Alpha Name"],[secondScope,"request_beta","Synthetic Beta Name"]]) {
      const item=released.find(result=>result.requestId===requestId),workspace=open(owner);
      try {
        assert.equal(workspace.restore(item.text),expected);
        const restored=restoreStoredResponse({store,scope:owner,requestId,text:item.text});
        assert.equal(restored.requestId,requestId);assert.equal(restored.text,expected);
      } finally {workspace.close();}
    }
    const alphaResult=released.find(result=>result.requestId==="request_alpha");
    assert.throws(()=>restoreStoredResponse({store,scope:secondScope,requestId:"request_alpha",text:alphaResult.text}),error=>error.code==="invalid_marker");
    await rejected(()=>run({tenantId:"unknown_tenant",workspaceId:"anything"},"denied_request","Synthetic History Name",113),"workspace_unavailable");
    authorized.set(scopeKey({...scope,workspaceId:"missing_history_only"}),key);
    await rejected(()=>run({...scope,workspaceId:"missing_history_only"},"history_only","Synthetic History Name",114),"workspace_unavailable");

    assert.throws(()=>restoreStoredResponse({store,scope:{...scope,workspaceId:"missing_history_only"},requestId:"history_response",text:alphaResult.text}),error=>error.code==="workspace_unavailable");

    // Opt-in progressive response delivery uses authorized durable mappings and a full head.
    const streamText=`👋 ${alphaResult.text}!`,streamBytes=new TextEncoder().encode(streamText),delivered=[];
    const streamRequest=(options={})=>restoreStoredResponseProgressively({store,scope,requestId:"stream_response",expectedHead:head(scope),chunks:[streamBytes.slice(0,6),streamBytes.slice(6)],deliver:async chunk=>delivered.push(chunk),...options});
    const complete=await streamRequest();
    assert.equal(complete.complete,true);assert.equal(delivered.map(chunk=>chunk.text).join(""),"👋 Synthetic Alpha Name!");
    assert(delivered.slice(0,-1).every(chunk=>chunk.complete===false));assert.equal(delivered.at(-1).complete,true);
    assert.equal(complete.delivered.utf16,"👋 Synthetic Alpha Name!".length);
    const staleHead={...head(scope),commitId:[...id(200)]};
    const deliveredBefore=delivered.length;
    await rejected(()=>streamRequest({expectedHead:staleHead}),"workspace_conflict");assert.equal(delivered.length,deliveredBefore);
    const partial=[];
    await assert.rejects(()=>streamRequest({chunks:[new TextEncoder().encode("safe "),new TextEncoder().encode("[[PERSON_AAAAAA]]")],deliver:async chunk=>partial.push(chunk)}),error=>{
      assert.equal(error.code,"invalid_marker");assert.equal(error.complete,false);assert.equal(error.delivered.bytes,5);assert.equal(error.markerIssues[0].kind,"unknown");assert(!("text" in error));return true;
    });
    assert.deepEqual(partial.map(chunk=>chunk.text),["safe "]);assert(partial.every(chunk=>!chunk.complete));
    const badEncoding=[];
    await assert.rejects(()=>streamRequest({chunks:[new TextEncoder().encode("safe "),new Uint8Array([0xff])],deliver:async chunk=>badEncoding.push(chunk)}),error=>error.code==="invalid_encoding" && error.complete===false && error.delivered.bytes===5);
    assert.deepEqual(badEncoding.map(chunk=>chunk.text),["safe "]);
    const cancelledStream=new AbortController(),cancelledChunks=[];
    await assert.rejects(()=>streamRequest({signal:cancelledStream.signal,chunks:[new TextEncoder().encode("safe "),streamBytes],deliver:async chunk=>{cancelledChunks.push(chunk);cancelledStream.abort();}}),error=>error.code==="cancelled" && error.complete===false && error.delivered.bytes===5);
    assert.deepEqual(cancelledChunks.map(chunk=>chunk.text),["safe "]);
    const pausedStream=new AbortController();let streamEntered,streamResume;
    const enteringStream=new Promise(resolve=>streamEntered=resolve),resumeStream=new Promise(resolve=>streamResume=resolve);
    const pausedChunks=[];
    const pendingStream=streamRequest({signal:pausedStream.signal,deliver:async chunk=>{streamEntered();await resumeStream;if(chunk.signal.aborted)throw new HostFailure("cancelled");pausedChunks.push(chunk);}});
    await enteringStream;pausedStream.abort();streamResume();
    await assert.rejects(()=>pendingStream,error=>error.code==="cancelled" && error.complete===false);assert.equal(pausedChunks.length,0);
    await assert.rejects(()=>streamRequest({deliver:async()=>{throw new Error("Synthetic private transport detail");}}),error=>error.code==="stream_delivery_failed" && error.complete===false && !error.message.includes("Synthetic"));
    async function* brokenSource(){yield new TextEncoder().encode("safe ");throw Object.assign(new Error("Synthetic private provider detail"),{complete:false,delivered:{private:"Synthetic"}});}
    await assert.rejects(()=>streamRequest({chunks:brokenSource()}),error=>error.code==="stream_input_failed" && error.complete===false && error.delivered.bytes===5 && !JSON.stringify(error).includes("Synthetic"));
    await assert.rejects(()=>streamRequest({encoding:"utf32"}),error=>error.code==="invalid_encoding" && error.complete===false && error.delivered.bytes===0);
    const sourceSignal=new AbortController();let sourceEntered;
    const enteringSource=new Promise(resolve=>sourceEntered=resolve);
    async function* pausedSource(){yield new TextEncoder().encode("safe ");sourceEntered();await new Promise(resolve=>sourceSignal.signal.addEventListener("abort",resolve,{once:true}));throw new Error("private cancelled provider detail");}
    const pendingSource=streamRequest({signal:sourceSignal.signal,chunks:pausedSource()});await enteringSource;sourceSignal.abort();
    await assert.rejects(()=>pendingSource,error=>error.code==="cancelled" && error.complete===false && error.delivered.bytes===5);
    await assert.rejects(()=>streamRequest({scope:{...scope,workspaceId:"missing_history_only"}}),error=>error.code==="workspace_unavailable" && error.complete===false);

    // Authenticated metadata changes cannot redirect an encrypted mapping row.
    const raw=new DatabaseSync(database),row=raw.prepare("SELECT * FROM workspaces WHERE tenant=? AND workspace=?").get(scope.tenantId,scope.workspaceId);
    raw.prepare("UPDATE workspaces SET revision=? WHERE tenant=? AND workspace=?").run("999",scope.tenantId,scope.workspaceId);
    assert.throws(()=>store.load(scope),error=>error.code==="invalid_snapshot");
    raw.prepare("UPDATE workspaces SET revision=? WHERE tenant=? AND workspace=?").run(row.revision,scope.tenantId,scope.workspaceId);
    const another=raw.prepare("SELECT * FROM workspaces WHERE tenant=? AND workspace=?").get(sameRevisionScope.tenantId,sameRevisionScope.workspaceId);
    const overwrite=record=>raw.prepare("UPDATE workspaces SET revision=?,commit_id=?,namespace=?,nonce=?,ciphertext=?,tag=? WHERE tenant=? AND workspace=?")
      .run(record.revision,record.commit_id,record.namespace,record.nonce,record.ciphertext,record.tag,scope.tenantId,scope.workspaceId);
    overwrite(another);
    assert.throws(()=>store.load(scope),error=>error.code==="invalid_snapshot","same key cannot decrypt a swapped workspace AAD");
    authorized.delete(scopeKey(scope));
    assert.throws(()=>store.load(scope),error=>error.code==="workspace_unavailable","authorization precedes decrypt even for corrupted ciphertext");
    authorized.set(scopeKey(scope),key);overwrite(row);raw.close();

    // Track update output before final authenticates, so a failed tag wipes it too.
    const corruptedTag=Buffer.from(row.tag);corruptedTag[0]^=1;
    const tamper=new DatabaseSync(database);
    tamper.prepare("UPDATE workspaces SET tag=? WHERE tenant=? AND workspace=?").run(corruptedTag,scope.tenantId,scope.workspaceId);
    const originalDecipher=crypto.createDecipheriv,capturedPlaintext=[];
    try {
      crypto.createDecipheriv=(...args)=>{
        const decipher=originalDecipher(...args),update=decipher.update.bind(decipher);
        decipher.update=(...input)=>{const value=update(...input);capturedPlaintext.push(value);return value;};
        return decipher;
      };
      syncBuiltinESMExports();
      assert.throws(()=>store.load(scope),error=>error.code==="invalid_snapshot");
      assert(capturedPlaintext.some(value=>value.length>0));
      assert(capturedPlaintext.every(value=>value.every(byte=>byte===0)),"authentication failure must wipe transient decrypted bytes");
    } finally {
      crypto.createDecipheriv=originalDecipher;syncBuiltinESMExports();
      for(const value of capturedPlaintext)value.fill(0);
      tamper.prepare("UPDATE workspaces SET tag=? WHERE tenant=? AND workspace=?").run(row.tag,scope.tenantId,scope.workspaceId);tamper.close();
    }

    for(const file of await readdir(directory)) {
      if(!file.startsWith("workspaces.sqlite"))continue;
      const bytes=await readFile(path.join(directory,file));
      for(const text of [value,"Synthetic Crash Name","Synthetic Alpha Name","Synthetic Beta Name"])assert(!bytes.includes(Buffer.from(text)),"database/WAL must contain encrypted state only");
      assert(!bytes.includes(key) && !bytes.includes(otherKey),"host keys must not be persisted in database/WAL");
    }
    return {progressiveComplete:true,progressiveLateFailure:true,progressiveCancellation:true,progressiveTransportFailure:true,stateChangingRaceWinners:1,stateChangingRaceLosers:1,noOpReleases:2,failedAuthenticationWiped:true,cooperativeReleaseCancellation:true,crashBefore:true,crashAfter:true,lostAck:true,cancellation:true,tenantIsolation:true,historyOnlyRejected:true};
  } finally {
    for(const {worker}of workers)await worker.terminate();
    store.close();key.fill(0);otherKey.fill(0);await rm(directory,{recursive:true,force:true});
  }
}
