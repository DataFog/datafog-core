// Validator regressions mutate actual retained native-host outputs; no mock saves.
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import {test} from "node:test";
import {validateConversationHostOutput} from "./native-conversation-host-profile.mjs";
const load=file=>JSON.parse(readFileSync(file,"utf8"));
if(process.argv.length!==6)throw new Error("four actual conversation-host receipt paths are required");
const [fixture,oracle,python,node]=process.argv.slice(2).map(load);
const scenario=(v,id)=>v.cases.find(value=>value.id===id);
const step=(v,id,call)=>scenario(v,id).steps.find(value=>value.call===call).value;
for(const [sdk,actual] of [["python",python],["node",node]]){
 test(`${sdk}: complete actual profile is accepted`,()=>validateConversationHostOutput(actual,fixture,oracle,sdk));
 const reject=(name,change)=>test(`${sdk}: ${name}`,()=>{const changed=structuredClone(actual);change(changed);assert.throws(()=>validateConversationHostOutput(changed,fixture,oracle,sdk));});
 reject("an unclean worker exit cannot be ignored",v=>{step(v,"process-pending-race","race")[0].processExit.code=1;});
 reject("a missing native scenario fails",v=>{v.cases.pop();});
 reject("unscoped applicability cannot replace native SDK identity",v=>{v.sdk="wasm";});
 reject("missing actual callback cannot confer confirmation",v=>{delete step(v,"observed-success","transport-observed").received;});
 reject("a mismatched received attempt cannot match a planned registry entry",v=>{step(v,"observed-success","transport-observed").received.preparationId.fill(99);});
 reject("a callback cannot substitute its own site witness",v=>{step(v,"observed-success","transport-observed").siteVerification.siteId="wrong_site";});
 reject("transport before actual pending persistence fails",v=>{step(v,"observed-success","transport-observed").durableBeforeCallback=oracle.states.seed;});
 reject("an uncertain callback is not successful send observation",v=>{step(v,"observed-success","transport-observed").received.kind="uncertain";});
 reject("a non-delivery resolution cannot claim a confirmation bundle",v=>{step(v,"definitely-not-sent","resolution-saved").candidateBundleHex=oracle.states.confirmedA.bundleHex;});
 reject("metadata retry cannot silently resend",v=>{scenario(v,"confirmation-save-rollback").trace.transportCalls=2;});
 reject("lost acknowledgment cannot activate local state",v=>{step(v,"confirmation-lost-ack","confirmation-uncertain").activated=true;});
 reject("an unresolved follow-up pauses older plain replies",v=>{step(v,"existing-chat-pause","still-paused").plain.code="invalid_marker";});
 reject("full parent commit ID remains mandatory",v=>{step(v,"full-parent-conflict","wrong-commit-id").expectedHead=oracle.states.pendingA.head;});
 reject("the actual race winner ID cannot be rewritten",v=>{const winner=step(v,"process-pending-race","race").find(x=>x.save.ok);winner.save.candidateHead.commitId.fill(99);});
 reject("the race acknowledgment must match the actual host",v=>{const winner=step(v,"process-confirmation-race","race").find(x=>x.save.ok);winner.save.acknowledgment=sdk==="node"?null:oracle.states.confirmedA.head;});
 reject("a losing pending save cannot dispatch",v=>{const loser=step(v,"process-pending-race","race").find(x=>!x.save.ok);loser.transportEvents=[step(v,"observed-success","transport-observed")];});
 reject("a crash receipt must retain the actual candidate head",v=>{step(v,"crash-confirmation-before","child-at-boundary").candidateHead=oracle.states.seed.head;});
 reject("a pending-save crash cannot invent an observed send",v=>{step(v,"crash-pending-after","child-at-boundary").events=[step(v,"observed-success","transport-observed")];});
 reject("a callback for another tenant is not silently rewritten",v=>{const entry=step(v,"context-and-tenant-isolation","foreign-callbacks");entry.callbacks[0].scope=entry.attempt.scope;});
}
