// Run against the packed WASM SDK in both a web page and the MV3 extension.
export async function verifyWorkspacePolicy(api, counters) {
  const check=(condition,message)=>{if(!condition)throw new Error(message);};
  const equal=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
  const id=value=>new Uint8Array(16).fill(value);
  const throws=(call,code)=>{try{call();}catch(error){check(error.code===code,`${error.code} != ${code}`);return;}throw new Error(`missing ${code}`);};
  const f=(type,value)=>({entityType:type,matchedText:value,range:{unit:"utf8_bytes",start:0,end:new TextEncoder().encode(value).length}});
  const edit=(type,value,disposition)=>({rules:[],choices:[{entityType:type,value,disposition}]});
  const save=(w,p)=>{const h=p.heads();w.activateSaved(p,h.expectedHead,h.candidateHead);};
  const w=api.Workspace.create(id(1),id(2));
  const initial=w.exportBundle();
  check(w.policy().snapshotVersion === 2,"new workspaces use candidate v2");
  check(w.policy().defaultRule === "hide" && w.policy().protectedTypes.includes("PERSON"),"Core policy descriptor");
  throws(()=>w.preparePolicyChanges(edit("PERSON","May","keep"),id(3)),"invalid_policy");
  throws(()=>w.preparePolicyChanges({rules:[{entityType:"PERSON",rule:"keep"}],choices:[]},id(3)),"invalid_policy");
  const unwaived=w.prepare("May",[{...f("PERSON","May"),source:"user",needsReview:false}],id(3));
  check(unwaived.preview().text !== "May","source metadata cannot authorize disclosure");unwaived.discard();
  let p=w.preparePolicyChanges(edit("PERSON","May","not_sensitive"),id(3));
  check(equal(w.exportBundle(),initial),"candidate settings are not committed");
  check(p.policyPreview().choices[0].before === null && p.policyPreview().choices[0].after === "not_sensitive","settings review preview");
  check(p.previews().length === 0,"policy plan is not a text handoff");
  const h=p.heads();throws(()=>w.activateSaved(p,h.expectedHead,h.expectedHead),"workspace_conflict");
  save(w,p);throws(()=>p.policyPreview(),"inactive_plan");
  p=w.prepare("MAY",[f("PERSON","MAY")],id(4));check(p.preview().text === "MAY","exact normalized correction before alias");p.discard();
  p=w.prepare("May",[f("EMAIL","May")],id(4));check(p.preview().text !== "May","type scope remains protected");p.discard();
  const copy=w.policy();copy.choices[0].disposition="keep";check(w.policy().choices[0].disposition === "not_sensitive","getter does not mutate settings");
  const bundle=w.exportBundle();const reopened=api.Workspace.openBundle(bundle);
  check(reopened.policy().choices[0].value === "May","choice survives reopen");
  p=reopened.preparePolicyChanges(edit("PERSON","May",null),id(4));save(reopened,p);
  p=reopened.prepare("May",[f("PERSON","May")],id(5));const marker=p.preview().text;save(reopened,p);
  p=reopened.preparePolicyChanges(edit("PERSON","May","not_sensitive"),id(6));save(reopened,p);
  check(reopened.recall("May").findings.length === 0,"recall obeys correction");
  check(reopened.restore(marker) === "May","old aliases stay permanent");
  p=reopened.preparePolicyChanges(edit("PERSON","MAY",null),id(7));save(reopened,p);
  check(reopened.recall("May").findings.length === 1,"removing correction restores recall");
  p=reopened.prepare("MAY",[f("PERSON","MAY")],id(8));check(p.preview().text === marker,"old alias reused");p.discard();
  p=reopened.preparePolicyChanges({rules:[{entityType:"PRODUCT_CODE",rule:"keep"}],choices:[]},id(8));save(reopened,p);
  p=reopened.prepareBatch([{text:"Alpha",findings:[f("PRODUCT_CODE","Alpha")]},{text:"Alpha",findings:[f("PERSON","Alpha")]}],id(9));
  check(p.previews()[0].text === "Alpha" && p.previews()[1].text !== "Alpha","Core policy is shared by all batch inputs");p.discard();
  const bad={...f("PERSON","May"),matchedText:"wrong"};throws(()=>w.prepare("May",[bad],id(4)),"invalid_finding");
  throws(()=>w.prepare("[[PERSON_ZZZZZZ]]",[],id(4)),"invalid_marker");
  const duplicate=edit("PERSON","Jane Smith","not_sensitive");duplicate.choices.push({entityType:"PERSON",value:"JANE\u00a0SMITH",disposition:null});
  throws(()=>w.preparePolicyChanges(duplicate,id(4)),"invalid_policy");
  for (const invalid of [
    {rules:[],choices:[],extra:true},
    edit("PERSON","May","notSensitive"),
    {rules:[],choices:[{entityType:"PERSON",value:"May",disposition:"not_sensitive",needsReview:false}]},
    edit("PERSON","\ud800","not_sensitive"),
    {rules:[],choices:new Array(1)},
  ]) throws(()=>w.preparePolicyChanges(invalid,id(4)),"invalid_policy");
  let getterRead=false;
  throws(()=>w.preparePolicyChanges({rules:[],choices:[{entityType:"PERSON",get value(){getterRead=true;return "May";},disposition:"not_sensitive"}]},id(4)),"invalid_policy");
  check(!getterRead,"transport rejects accessors before invocation");
  throws(()=>w.preparePolicyChanges({rules:[],choices:Array(10001).fill({entityType:"PERSON",value:"May",disposition:null})},id(4)),"resource_limit");
  const legacyBytes=Uint8Array.from(counters.high.bundleHex.match(/../g),value=>parseInt(value,16));
  const legacy=api.Workspace.openBundle(legacyBytes);check(equal(legacy.exportBundle(),legacyBytes),"legacy read/export byte preservation");
  const noOp=legacy.prepare("ordinary",[],id(201));
  check(noOp.snapshotFormat().currentVersion === 1 && noOp.snapshotFormat().candidateVersion === 1,"legacy no-op does not upgrade");noOp.discard();
  p=legacy.preparePolicyChanges(edit("PERSON","Jane","not_sensitive"),id(201));
  check(p.snapshotFormat().currentVersion === 1 && p.snapshotFormat().candidateVersion === 2,"upgrade metadata supports warning before save");
  const candidate=api.Workspace.openBundle(p.exportCandidateBundle());
  check(candidate.restore(counters.high.marker) === "Jane" && candidate.recall("Jane").findings.length === 0,"staged upgrade keeps aliases and settings");
  p.discard();check(equal(legacy.exportBundle(),legacyBytes),"discarded upgrade preserves legacy bytes");
  const pending=reopened.preparePolicyChanges(edit("PERSON","May","not_sensitive"),id(10));reopened.close();throws(()=>pending.policyPreview(),"closed_workspace");
  w.close();legacy.close();candidate.close();
}
