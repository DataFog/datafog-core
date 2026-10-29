// Synthetic transport qualification only: no provider parser or SDK exports.
import assert from "node:assert/strict";
import {mkdtemp,rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import path from "node:path";
import {randomBytes} from "node:crypto";
import {Workspace} from "@datafog/node";
import {EncryptedWorkspaceStore,HostFailure,runPreparedRequest,restoreStoredResponse,restoreStoredResponseProgressively} from "./node-workspace-host.mjs";

export async function verifyNodeToolEnvelope() {
  const directory=await mkdtemp(path.join(tmpdir(),"datafog-tool-envelope-")),key=randomBytes(32);
  const owners=[{tenantId:"tenant_a",workspaceId:"workspace_a"},{tenantId:"tenant_a",workspaceId:"workspace_b"}];
  const sameScope=(a,b)=>a.tenantId===b.tenantId && a.workspaceId===b.workspaceId;
  const store=new EncryptedWorkspaceStore(path.join(directory,"state.sqlite"),owner=>owners.some(item=>sameScope(item,owner))?key:undefined);
  const id=value=>new Uint8Array(16).fill(value),value="Synthetic Shared Person";
  const unknown="[[PERSON_AAAAAA]]",opaqueArguments='{\n "literal": "[[PERSON_AAAAAA]]", "name": "Opaque Tool Person", "emoji": "👋"\n}';
  const request=text=>({id:"untrusted_provider_id",metadata:{tenantId:"forged",workspaceId:"forged",requestId:"forged"},
    tools:[{name:"opaque_tool",input_schema:{type:"object",properties:{literal:{const:unknown}}}}],
    rawToolArguments:opaqueArguments,messages:[{role:"user",content:[{type:"text",text},{type:"tool_result",tool_use_id:unknown,content:"Opaque Tool Person"}]}]});
  const response=text=>({id:"another_provider_id",metadata:{tenantId:"forged",workspaceId:"forged",requestId:"forged"},
    rawToolArguments:opaqueArguments,content:[{type:"text",text},{type:"tool_use",id:unknown,name:"opaque_tool",input:{literal:unknown,name:"Opaque Tool Person"}}]});
  const textOf=envelope=>envelope.messages ? envelope.messages[0].content[0].text : envelope.content[0].text;
  const replace=(envelope,text)=>{const copy=structuredClone(envelope);if(copy.messages)copy.messages[0].content[0].text=text;else copy.content[0].text=text;return copy;};
  const opaque=envelope=>JSON.stringify(replace(envelope,"<designated text slot>"));
  // Real hosts must obtain this association from their trusted request/transport
  // registry. Provider fields never construct or select either capability.
  const channels=new WeakMap(),capabilities=new WeakMap();let correlationRejects=0;
  const authorize=(channel,capability,owner)=>{
    const context=channels.get(channel);
    if(!context || capabilities.get(capability)!==context || !sameScope(context.scope,owner)) {
      correlationRejects++;throw new HostFailure("invalid_response_context");
    }
    return context;
  };
  async function prepare(owner,requestId,operation) {
    const original=request(value);let sent,channel,capability;
    await runPreparedRequest({store,scope:owner,requestId,operationId:id(operation),
      input:{text:textOf(original),scannerFindings:[],userSelections:[{entityType:"PERSON",matchedText:value,range:{unit:"utf8_bytes",start:0,end:Buffer.byteLength(value)}}]},
      approve:async()=>true,release:async released=>{
        sent=replace(original,released.text);
        const context={scope:Object.freeze({...owner}),requestId,head:structuredClone(released.head)};
        channel=Object.freeze({});capability=Object.freeze({});channels.set(channel,context);capabilities.set(capability,context);
      }});
    assert.equal(opaque(sent),opaque(original));assert.equal(sent.rawToolArguments,opaqueArguments);
    return {owner,channel,capability,sent};
  }
  function strict(item,envelope,capability=item.capability,owner=item.owner) {
    const context=authorize(item.channel,capability,owner);
    const restored=restoreStoredResponse({store,scope:context.scope,requestId:context.requestId,text:textOf(envelope)});
    const output=replace(envelope,restored.text);assert.equal(opaque(output),opaque(envelope));return output;
  }
  async function progressive(item,envelope,chunks,deliver,options={}) {
    const context=authorize(item.channel,options.capability??item.capability,options.owner??item.owner);
    const before=opaque(envelope);
    try {return await restoreStoredResponseProgressively({store,scope:context.scope,requestId:context.requestId,expectedHead:context.head,chunks,deliver,signal:options.signal});}
    finally {assert.equal(opaque(envelope),before);assert.equal(envelope.rawToolArguments,opaqueArguments);}
  }
  try {
    for(const [index,owner] of owners.entries()) {
      const workspace=Workspace.create(id(index+1),id(index+3)),bytes=workspace.exportBundle();
      try {store.initialize(owner,bytes);}finally{bytes.fill(0);workspace.close();}
    }
    // Equal values in simultaneous requests are not a correlation key.
    const [a,b]=await Promise.all([prepare(owners[0],"request_a",10),prepare(owners[1],"request_b",11)]);
    for(const item of [a,b])assert.equal(textOf(strict(item,response(textOf(item.sent)))),value);
    assert.throws(()=>strict(a,response(textOf(a.sent)),b.capability),error=>error.code==="invalid_response_context");
    assert.throws(()=>strict(a,response(textOf(a.sent)),a.capability,b.owner),error=>error.code==="invalid_response_context");
    assert.throws(()=>strict(a,response(textOf(a.sent)),Object.freeze({})),error=>error.code==="invalid_response_context");
    // Same workspace and same aliases still require distinct trusted request routes.
    const [followA,followB]=await Promise.all([prepare(owners[0],"follow_a",12),prepare(owners[0],"follow_b",13)]);
    assert.equal(textOf(followA.sent),textOf(followB.sent));
    assert.throws(()=>strict(followA,response(textOf(followA.sent)),followB.capability),error=>error.code==="invalid_response_context");
    for(const item of [followA,followB])assert.equal(textOf(strict(item,response(textOf(item.sent)))),value);
    const opened=store.load(owners[0]);let workspace;
    try {workspace=Workspace.openBundle(opened.bundle);assert.equal(workspace.recall("Opaque Tool Person").findings.length,0);}
    finally {opened.bundle.fill(0);workspace?.close();}

    const invalid=response(`safe ${unknown}`),unchanged=JSON.stringify(invalid);
    assert.throws(()=>strict(a,invalid),error=>error.code==="invalid_marker" && !("text" in error));
    assert.equal(JSON.stringify(invalid),unchanged,"strict failure publishes no partially restored envelope");

    const envelope=response(textOf(a.sent)),text=`👋 ${textOf(a.sent)}!`,bytes=new TextEncoder().encode(text),delivered=[];
    const complete=await progressive(a,envelope,[bytes.slice(0,7),bytes.slice(7)],async chunk=>delivered.push(chunk));
    assert.equal(complete.complete,true);assert.equal(delivered.map(chunk=>chunk.text).join(""),`👋 ${value}!`);
    assert(delivered.every(chunk=>chunk.requestId==="request_a"));
    const safe=new TextEncoder().encode("safe 👋 "),partial=[];
    await assert.rejects(()=>progressive(a,envelope,[safe,new TextEncoder().encode(unknown)],async chunk=>partial.push(chunk)),error=>{
      assert.equal(error.code,"invalid_marker");assert.equal(error.complete,false);
      assert.deepEqual(error.delivered,{bytes:10,codepoints:7,utf16:8,restoredMarkers:0});assert(!("text" in error));return true;
    });
    assert.deepEqual(partial.map(chunk=>chunk.text),["safe 👋 "]);
    const controller=new AbortController(),cancelled=[];
    await assert.rejects(()=>progressive(a,envelope,[safe,bytes],async chunk=>{cancelled.push(chunk);controller.abort();},{signal:controller.signal}),error=>error.code==="cancelled" && error.complete===false && error.delivered.bytes===10);
    assert.deepEqual(cancelled.map(chunk=>chunk.text),["safe 👋 "]);
    let emitted=0;
    await assert.rejects(()=>progressive(a,envelope,[bytes],async()=>{emitted++;},{capability:b.capability}),error=>error.code==="invalid_response_context");
    assert.equal(emitted,0);
    return {scope:"synthetic explicitly designated text slots only; Node installed SDK/SQLite, not provider translation or Python acceptance",
      concurrentEqualValues:true,sameWorkspaceRequestIsolation:true,correlationRejects,opaqueToolMetadataUnchanged:true,
      strictNoPartialEnvelope:true,progressiveComplete:true,progressiveIncomplete:true,cancellation:true};
  } finally {store.close();key.fill(0);await rm(directory,{recursive:true,force:true});}
}
