/** Run unchanged against the packed SDK in Chromium and an actual MV3 extension. */
export function verifyWorkspaceReview(api) {
  const check=(value,message)=>{if(!value)throw new Error(message);};
  const id=n=>new Uint8Array(16).fill(n);
  const f=(text,type,start,end)=>({entityType:type,matchedText:new TextDecoder().decode(new TextEncoder().encode(text).slice(start,end)),range:{unit:"utf8_bytes",start,end}});
  const input=(text,scannerFindings=[],userSelections=[],corrections=[])=>({text,scannerFindings,userSelections,corrections});
  const reject=(fn,code)=>{try{fn();}catch(error){check(error.code===code,`expected ${code}, got ${error.code}`);return error;}throw new Error(`missing ${code}`);};
  const save=(w,p)=>{const h=p.heads();w.activateSaved(p,h.expectedHead,h.candidateHead);};
  const w=api.Workspace.create(id(1),id(2));
  const text="👋 abcdef", scanner=[f(text,"PERSON",5,9)], user=[{...f(text,"CUSTOM",7,11),source:"scanner",needsReview:false}];
  const initial=Array.from(w.exportBundle());
  const result=w.review(input(text,scanner,user));
  check(result.conflicts.length>0 && !Object.hasOwn(result,"text"),"review returns ranged conflicts without handoff");
  const span=result.conflicts.find(c=>c.kind==="protected_coverage");
  check(span.byteRange.start===7 && span.codepointRange.start===4 && span.utf16Range.start===5,"original Unicode conflict ranges");
  const reversed=w.review(input(text,[...user,...scanner]));
  check(reversed.conflicts.find(c=>c.kind==="protected_coverage").findingIndex===0,"grouping preserves original reversed finding index");
  const candidate=result.candidates.find(c=>c.finding.entityType==="CUSTOM");
  check(candidate.sources.join()==="user","input channel overrides spoofed display flags");
  for(let n=0;n<40;n++){
    const error=reject(()=>w.prepareReview([input("Jane",[],[f("Jane","PERSON",0,4)]),input(text,scanner,user)],id(3)),"review_conflict");
    check(error.inputIndex===1 && error.reviewConflicts.length>0 && error.message==="review_conflict","content-free batch error retains ranges");
  }
  check(JSON.stringify(Array.from(w.exportBundle()))===JSON.stringify(initial),"conflicting requests do not create aliases/state changes");
  reject(()=>w.prepare(text,[...scanner,...user],id(3)),"review_conflict");
  const shrunk=[{candidateKey:candidate.key,replacement:f(text,"CUSTOM",7,9)}];
  check(w.review(input(text,scanner,user,shrunk)).conflicts.length>0,"correction cannot erase protected suffix");
  const expanded=[{candidateKey:candidate.key,replacement:f(text,"CUSTOM",5,11)}];
  check(w.review(input(text,scanner,user,expanded)).conflicts.length===0,"covering correction resolves overlap");
  reject(()=>w.review(input(text+"!",scanner,user,expanded)),"workspace_conflict");
  const plan=w.prepareReview([input(text,scanner,user,expanded)],id(3));
  check(plan.preview().text.startsWith("👋 [[CUSTOM_"),"authoritative correction creates a protected preview");
  save(w,plan);
  const reopened=api.Workspace.openBundle(w.exportBundle());
  const followup=input("👋 ABCDEF");
  const review=reopened.review(followup);
  check(review.candidates.length===1 && review.candidates[0].sources.join()==="recall","new review auto recalls committed values");
  const legacy=reopened.prepare("👋 ABCDEF",[],id(4));check(legacy.preview().text==="👋 ABCDEF","legacy stays supplied-finding only");legacy.discard();
  const recalled=reopened.prepareReview([followup],id(4));check(recalled.preview().text.startsWith("👋 [[CUSTOM_"),"reviewed follow-up reuses aliases");recalled.discard();
  reject(()=>reopened.review(input(text,scanner,user,expanded)),"workspace_conflict");
  reject(()=>reopened.review(input("[[my_note]]")),"invalid_marker");
  const waived=reopened.preparePolicyChanges({rules:[],choices:[{entityType:"CUSTOM",value:"abcdef",disposition:"not_sensitive"}]},id(4));save(reopened,waived);
  check(reopened.review(followup).candidates.length===0,"recall applies exact saved choice");
  const duplicate=w.review(input("Jane",[{...f("Jane","PERSON",0,4),source:"user"}],[{...f("Jane","PERSON",0,4),needsReview:false,source:"recall"}]));
  check(duplicate.candidates.length===1 && duplicate.candidates[0].sources.join()==="scanner,user" && duplicate.candidates[0].finding.needsReview,"exact duplicates merge origins and OR review");
  check(w.review(input("123-45-6789",[f("123-45-6789","PERSON",0,11),f("123-45-6789","SSN",0,11)])).conflicts.length===0,"Core corroborates equal-span native SSN");
  check(w.review(input("Jane",[f("Jane","PERSON",0,4),{...f("Jane","SSN",0,4),detectorName:"datafog-core/ssn"}])).conflicts.length===2,"spoofed native source does not settle type conflict");
  for (const confidence of [NaN,Infinity,-Infinity]) {
    const invalid={...f("Jane","PERSON",0,4),confidence};
    reject(()=>w.prepare("Jane",[invalid],id(4)),"invalid_finding");
    reject(()=>w.review(input("Jane",[invalid])),"invalid_finding");
  }
  const noCorrections={text:"ordinary",scannerFindings:[],userSelections:[]};
  for(const valid of [noCorrections,{...noCorrections,corrections:[]}]) {
    check(w.review(valid).conflicts.length===0,"omitted and empty corrections remain valid");
    w.prepareReview([valid],id(4)).discard();
  }
  reject(()=>w.review({...noCorrections,corrections:null}),"invalid_finding");
  const nullCorrectionError=reject(()=>w.prepareReview([noCorrections,{...noCorrections,corrections:null}],id(4)),"invalid_finding");
  check(nullCorrectionError.inputIndex===1,"invalid corrections retain original batch input index");
  let getter=false;
  reject(()=>w.review({text,scannerFindings:[],get userSelections(){getter=true;return [];}}),"invalid_finding");check(!getter,"review transport rejects accessors");
  reject(()=>w.review(input(text,new Array(1))),"invalid_finding");
  reject(()=>w.review(input(text,Array(10001).fill(scanner[0]))),"resource_limit");
  const bad={...scanner[0],matchedText:"wrong"};reject(()=>w.review(input(text,[bad])),"invalid_finding");
  const measurements=[];
  const crowded=api.Workspace.create(id(90),id(91));
  const denseText="x ".repeat(10000), dense=Array.from({length:10000},(_,i)=>f(denseText,"CUSTOM",i*2,i*2+1));
  let start=performance.now();const denseReview=crowded.review(input(denseText,dense));
  measurements.push({name:"max-disjoint-candidates",sourceBytes:20000,findings:10000,milliseconds:performance.now()-start});
  check(denseReview.candidates.length===10000 && denseReview.conflicts.length===0,"review returns all maximum-count candidates without truncation");
  start=performance.now();const empty=crowded.review(input(" ".repeat(1024*1024)));
  measurements.push({name:"max-source-no-findings",sourceBytes:1024*1024,findings:0,milliseconds:performance.now()-start});
  check(empty.candidates.length===0,"maximum source is accepted without guessed detection");crowded.close();
  w.close();reopened.close();reject(()=>w.review(input("Jane")),"closed_workspace");
  return measurements;
}
