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
