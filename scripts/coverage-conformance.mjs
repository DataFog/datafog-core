// Valid crossing claims must fail before any transformed text or provider work.
export function verifyCoverageConflicts(api) {
  const text = "👋 abcdef";
  const finding = (entityType, start, end) => ({entityType,
    matchedText: Array.from(text).slice(start, end).join(""),
    byteRange: {start: new TextEncoder().encode(Array.from(text).slice(0,start).join("")).length, end: new TextEncoder().encode(Array.from(text).slice(0,end).join("")).length},
    codepointRange: {start,end}, detectorName: "synthetic"});
  const first = finding("PERSON",2,6), second = finding("CUSTOM",4,8);
  function rejects(operation, index, code = "review_conflict") {
    try { operation(); } catch (error) {
      if (error.code !== code || error.findingIndex !== index || String(error.message).includes("abcdef")) throw new Error("unsafe or incorrect coverage error");
      return;
    }
    throw new Error("partial coverage returned a successful result");
  }
  const config = {default:{strategy:"redact"}};
  for (const inputs of [[first,second],[second,first]]) {
    const index = inputs.indexOf(second);
    rejects(() => api.transform(text,inputs,config),index);
    for (const strategy of [{strategy:"pseudonymize",key_ref:"unused"},{strategy:"tokenize",token_ref:"unused"}]) {
      rejects(() => api.transform(text,inputs,{default:strategy}),index);
    }
    const data = {a:"plain",z:text};
    const prefix = {entityType:"CUSTOM",matchedText:"plain",byteRange:{start:0,end:5},codepointRange:{start:0,end:5},detectorName:"synthetic"};
    rejects(() => api.transformStructured(data,[{path:"/a",finding:prefix},...inputs.map(finding=>({path:"/z",finding}))],config),index+1);
  }
  const allowed = api.transform(text,[first,second],{...config,allow:{exact:{PERSON:["abcd"]}}});
  if (allowed.text !== "👋 ab[CUSTOM]") throw new Error("explicit allowlist changed");
  const selected = api.transform(text,[first,second],{...config,entities:["PERSON"]});
  if (selected.text !== "👋 [PERSON]ef") throw new Error("explicit selection changed");
  const outer = finding("PERSON",2,8);
  if (api.transform(text,[outer,second],config).text !== "👋 [PERSON]") throw new Error("whole-cover containment changed");
  rejects(() => api.transform(text,[first,{...second,matchedText:"wrong"}],{...config,entities:["PERSON"]}),1,"invalid_finding");
}

export async function verifyCoverageProviderCalls(api) {
  let calls = 0;
  const unavailable = async () => { calls++; throw new Error("provider must not run"); };
  const manager = new api.PrivacyManager({keyProvider:{resolveKey:unavailable},tokenProvider:{tokenizeBatch:unavailable,restoreBatch:unavailable}});
  const text = "abcdef";
  const findings = [
    {entityType:"PERSON",matchedText:"abcd",byteRange:{start:0,end:4},codepointRange:{start:0,end:4},detectorName:"synthetic"},
    {entityType:"CUSTOM",matchedText:"cdef",byteRange:{start:2,end:6},codepointRange:{start:2,end:6},detectorName:"synthetic"},
  ];
  for (const strategy of [{strategy:"pseudonymize",key_ref:"unused"},{strategy:"tokenize",token_ref:"unused"}]) {
    for (const operation of [
      () => manager.transform(text,findings,{default:strategy},{scope:"synthetic"}),
      () => manager.transformStructured({z:text},findings.map(finding=>({path:"/z",finding})),{default:strategy},{scope:"synthetic"}),
    ]) {
      let failed = false;
      try { await operation(); } catch (error) { if(error.code !== "review_conflict") throw error; failed = true; }
      if (!failed) throw new Error("provider-backed conflict returned output");
    }
  }
  if (calls !== 0) throw new Error("a provider ran before conflict rejection");
}
