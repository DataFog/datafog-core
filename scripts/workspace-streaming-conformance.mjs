// Shared public-package protocol. Parser decisions and expected counts live in Core/fixtures.
function assert(value, message) { if (!value) throw new Error(message); }
function canonical(value) { if (Array.isArray(value)) return value.map(canonical); if (value && typeof value === "object") return Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])); return value; }
function equal(actual, expected, message) { assert(JSON.stringify(canonical(actual)) === JSON.stringify(canonical(expected)), message); }
const id = byte => new Uint8Array(16).fill(byte);
function seed(api) {
  const workspace=api.Workspace.create(id(1),id(2));
  const plan=workspace.prepare("Jane",[{entityType:"PERSON",matchedText:"Jane",range:{unit:"utf8_bytes",start:0,end:4}}],id(3));
  const marker=plan.preview().text;
  workspace.activateSaved(plan,plan.heads().expectedHead,plan.heads().candidateHead);
  return {workspace,marker};
}
function counts(text,restoredMarkers) {return {bytes:new TextEncoder().encode(text).length,codepoints:[...text].length,utf16:text.length,restoredMarkers};}
function failure(call,code) {
  try {call();} catch(error) {assert(error.code===code,"stream error code differs");return error;}
  throw new Error("stream call unexpectedly succeeded");
}
export async function verifyWorkspaceStreaming(api,fixture) {
  if(api.init) await api.init();
  assert(fixture.schemaVersion===1 && fixture.profile==="workspace-streaming-v1","unsupported streaming fixture");
  assert(typeof api.Workspace.prototype.bufferedRestore==="function" && typeof api.Workspace.prototype.progressiveRestore==="function","missing streaming runtime method");
  const results=[];
  for(const scenario of fixture.cases) {
    const {workspace,marker}=seed(api);
    const before=workspace.exportBundle();
    const stream=workspace.progressiveRestore(scenario.encoding);
    const steps=[];
    try {
      for(const step of scenario.steps) {
        let result,error;
        try {
          switch(step.op) {
            case "feed": result=stream.feed(step.bytes ? new Uint8Array(step.bytes) : new TextEncoder().encode(step.text.replaceAll("{marker}",marker).replaceAll("{bareMarker}",marker.slice(2,-2)).replaceAll("{oversizedPrefix}",`[[PERSON_${"A".repeat(100)}`)));break;
            case "feedUtf16":result=stream.feedUtf16(new Uint16Array(step.units));break;
            case "finalize":result=stream.finalize();break;
            case "reset":stream.reset();break;
            case "closeStream":stream.close();break;
            case "closeWorkspace":workspace.close();break;
            case "advanceWorkspace": {
              const plan=workspace.prepare("Bob",[{entityType:"PERSON",matchedText:"Bob",range:{unit:"utf8_bytes",start:0,end:3}}],id(4));
              workspace.activateSaved(plan,plan.heads().expectedHead,plan.heads().candidateHead);break;
            }
            default:throw new Error("unsupported streaming command");
          }
        } catch(caught) {error=caught;}
        if(step.expect) {
          assert(!error,`streaming ${scenario.id}/${step.op} failed`);
          const actual={text:result.text,delivered:result.delivered,complete:result.complete};
          equal(actual,{text:step.expect.text,delivered:counts(step.expect.deliveredText,step.expect.restoredMarkers),complete:step.expect.complete},`streaming ${scenario.id}/${step.op} differs`);
          steps.push({ok:actual});
        } else if(step.expectError) {
          assert(error instanceof api.DataFogError,`streaming ${scenario.id}/${step.op} must throw DataFogError`);
          const expected=step.expectError;
          assert(error.code===expected.code && error.complete===false,`streaming ${scenario.id}/${step.op} terminal differs`);
          equal(error.delivered,counts(expected.deliveredText,expected.restoredMarkers),"delivered counts differ");
          if(expected.issueKind) assert(error.markerIssues?.[0]?.kind===expected.issueKind,"marker classification differs");
          if(expected.markerIssues) equal(error.markerIssues,expected.markerIssues,"original Unicode marker ranges differ");
          assert(!("text" in error),"stream failure exposed text");
          assert(!JSON.stringify(error).includes("Jane"),"stream error exposed a private value");
          steps.push({error:{code:error.code,delivered:error.delivered,complete:error.complete,markerIssues:error.markerIssues}});
        } else {assert(!error,`streaming ${scenario.id}/${step.op} failed`);steps.push({ok:null});}
      }
      if(scenario.strictR2) {
        const atomic=workspace.bufferedRestore();atomic.feed(new TextEncoder().encode(scenario.strictR2));
        equal(atomic.finalize(),workspace.restoreReport(scenario.strictR2),"strict buffered R2 changed");atomic.close();
      }
      if(!scenario.steps.some(step=>step.op==="advanceWorkspace"||step.op==="closeWorkspace")) equal([...workspace.exportBundle()],[...before],"streaming mutated workspace");
      results.push({id:scenario.id,steps});
    } finally {stream.close();workspace.close();}
  }
  // Public transport rejects coercion and terminates the response with exact prior counts.
  for(const invalid of [null,[],[65],"a",{},new Uint16Array([65])]) {
    const {workspace}=seed(api);const stream=workspace.progressiveRestore();
    const previous=stream.feed(new TextEncoder().encode("safe ")).delivered;
    const error=failure(()=>stream.feed(invalid),"invalid_encoding");equal(error.delivered,previous,"transport failure lost delivered counts");assert(error.complete===false,"transport error must be incomplete");failure(()=>stream.finalize(),"inactive_stream");stream.close();workspace.close();
  }
  const {workspace,marker}=seed(api);
  for(const invalid of [null,"utf32",{},0]) {failure(()=>workspace.bufferedRestore(invalid),"invalid_encoding");failure(()=>workspace.progressiveRestore(invalid),"invalid_encoding");}
  for(const source of [`👋 ${marker}!`,`${marker}${marker}`,marker.toLowerCase(),"literal [[PERSON_","ordinary [[plain]] text"]) {
    const expected=workspace.restoreReport(source);
    const bytes=new TextEncoder().encode(source);
    for(let split=0;split<=bytes.length;split++) {
      const stream=workspace.bufferedRestore();assert(stream.feed(bytes.slice(0,split))===undefined,"buffered feed released output");assert(stream.feed(bytes.slice(split))===undefined,"buffered feed released output");equal(stream.finalize(),expected,"buffered split differs");stream.close();
      if(expected.text!==null && !source.endsWith("[[PERSON_")) {
        const live=workspace.progressiveRestore();let text=live.feed(bytes.slice(0,split)).text+live.feed(bytes.slice(split)).text;const end=live.finalize();text+=end.text;assert(text===expected.text && end.complete,"progressive split differs");live.close();
      }
    }
    const units=new Uint16Array(Array.from({length:source.length},(_,i)=>source.charCodeAt(i)));
    for(let split=0;split<=units.length;split++) {
      const atomic=workspace.bufferedRestore("utf16");atomic.feedUtf16(units.slice(0,split));atomic.feedUtf16(units.slice(split));equal(atomic.finalize(),expected,"UTF16 atomic split differs");atomic.close();
      if(expected.text!==null && !source.endsWith("[[PERSON_")) {
        const live=workspace.progressiveRestore("utf16");let text=live.feedUtf16(units.slice(0,split)).text+live.feedUtf16(units.slice(split)).text;const end=live.finalize();text+=end.text;assert(text===expected.text && end.complete,"UTF16 progressive split differs");live.close();
      }
    }
  }
  workspace.close();
  return results;
}

// Cross-SDK capture deliberately includes both modes. A strict replay may succeed
// at a step where the progressive fixture correctly failed (e.g. literal EOF).
export async function runWorkspaceStreamingContract(api,fixture) {
  await verifyWorkspaceStreaming(api,fixture);
  const cases=[];
  for(const scenario of fixture.cases) {
    const modes={};
    for(const mode of ["buffered","progressive"]) {
      const {workspace,marker}=seed(api);
      const stream=mode==="buffered"?workspace.bufferedRestore(scenario.encoding):workspace.progressiveRestore(scenario.encoding);
      const steps=[];
      try {
        for(const step of scenario.steps) {
          let value;
          try {
            switch(step.op) {
              case "feed":value=stream.feed(step.bytes?new Uint8Array(step.bytes):new TextEncoder().encode(step.text.replaceAll("{marker}",marker).replaceAll("{bareMarker}",marker.slice(2,-2)).replaceAll("{oversizedPrefix}",`[[PERSON_${"A".repeat(100)}`)));break;
              case "feedUtf16":value=stream.feedUtf16(new Uint16Array(step.units));break;
              case "finalize":value=stream.finalize();break;
              case "reset":stream.reset();break;
              case "closeStream":stream.close();break;
              case "closeWorkspace":workspace.close();break;
              case "advanceWorkspace": {
                const plan=workspace.prepare("Bob",[{entityType:"PERSON",matchedText:"Bob",range:{unit:"utf8_bytes",start:0,end:3}}],id(4));
                workspace.activateSaved(plan,plan.heads().expectedHead,plan.heads().candidateHead);break;
              }
              default:throw new Error("unsupported streaming command");
            }
            steps.push({op:step.op,ok:true,value:value??null});
          } catch(error) {
            if(!(error instanceof api.DataFogError) || error.message!==error.code) throw new Error("streaming SDK returned an unstructured error");
            steps.push({op:step.op,ok:false,error:{code:error.code,findingIndex:error.findingIndex??null,inputIndex:error.inputIndex??null,markerIssues:error.markerIssues??[],reviewConflicts:error.reviewConflicts??[],delivered:error.delivered??null,complete:error.complete??null}});
          }
        }
      } finally {stream.close();workspace.close();}
      modes[mode]={steps};
    }
    cases.push({id:scenario.id,modes});
  }
  return {schemaVersion:1,profile:fixture.profile,cases};
}
