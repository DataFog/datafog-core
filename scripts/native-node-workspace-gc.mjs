import assert from "node:assert/strict";
import {Workspace} from "@datafog/node";
import {setImmediate} from "node:timers/promises";
assert.equal(typeof globalThis.gc,"function");
const id=value=>new Uint8Array(16).fill(value);
const workspace=Workspace.create(id(1),id(2));
for(let index=0;index<32;index++) workspace.prepare("",[],id(20+index));
assert.throws(()=>workspace.prepare("",[],id(100)),error=>error.code==="resource_limit");
// Bounded retries account for asynchronous finalizer scheduling, never skip proof.
let recovered=false;
for(let attempt=0;attempt<20;attempt++) {
  globalThis.gc();await setImmediate();await setImmediate();
  try {workspace.prepare("",[],id(100)).discard();recovered=true;break;}
  catch(error) {if(error.code!=="resource_limit")throw error;}
}
assert.equal(recovered,true,"GC must release orphaned native plan slots");
workspace.close();

// A retained child must not keep the workspace capability alive after owner GC.
let owner=Workspace.create(id(120),id(121));
const buffered=owner.bufferedRestore(), progressive=owner.progressiveRestore();
buffered.feed(new TextEncoder().encode("private pending"));
progressive.feed(new TextEncoder().encode("safe "));
owner=null;
let closed=false;
for(let attempt=0;attempt<30;attempt++) {
  globalThis.gc();await setImmediate();await setImmediate();
  try {progressive.feed(new Uint8Array());}
  catch(error) {if(error.code!=="closed_workspace")throw error;closed=true;break;}
}
assert.equal(closed,true,"owner GC closes retained native stream state");
assert.throws(()=>buffered.finalize(),error=>error.code==="closed_workspace");
progressive.close();buffered.close();
