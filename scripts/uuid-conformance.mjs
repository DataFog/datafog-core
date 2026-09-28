// The same fixture contract is exercised by installed Node and browser packages.
export function verifyUuid(api, records) {
  const equal = (a,b) => { if (JSON.stringify(a)!==JSON.stringify(b)) throw new Error(`UUID contract: ${JSON.stringify(a)} != ${JSON.stringify(b)}`); };
  for (const row of records) {
    const findings=api.scan(row.text,row.config);
    const uuids=findings.filter(f=>f.entityType==='UUID');
    equal(uuids.map(f=>({text:f.matchedText,start:f.codepointRange.start,end:f.codepointRange.end})),row.matches);
    for (const f of uuids) {
      equal(row.text.slice(f.utf16Range.start,f.utf16Range.end),f.matchedText);
      equal(new TextDecoder().decode(new TextEncoder().encode(row.text).slice(f.byteRange.start,f.byteRange.end)),f.matchedText);
      equal(Array.from(row.text).slice(f.codepointRange.start,f.codepointRange.end).join(''),f.matchedText);
      equal(f.detectorName,'datafog-core/uuid'); equal(f.confidence,undefined);
      if(!f.detectorVersion) throw new Error('UUID missing detector version');
    }
    const data={'a/b':row.text,items:[row.text]};
    const located=api.scanStructured(data,row.config).findings;
    for(const path of ['/a~1b','/items/0']) equal(located.filter(f=>f.path===path).map(f=>f.finding),findings);
    for(const strategy of ['redact','mask','remove']) {
      const config={default:{strategy},entities:['UUID']};
      const result=api.scanAndTransform(row.text,{scan:row.config,transform:config});
      let expected=Array.from(row.text);
      for(const match of [...row.matches].reverse()) expected.splice(match.start,match.end-match.start,...Array.from(strategy==='redact'?'[UUID]':strategy==='mask'?'*'.repeat(36):''));
      equal(result.text,expected.join(''));
      equal(api.transform(row.text,findings,config),result);
      for(const t of result.transformations) {
        equal(row.text.slice(t.sourceUtf16Range.start,t.sourceUtf16Range.end).length,36);
        equal(result.text.slice(t.outputUtf16Range.start,t.outputUtf16Range.end),t.replacement);
        if('matchedText' in t||'finding' in t) throw new Error('UUID transformation leaks source');
      }
      const structured=api.scanAndTransformStructured(data,{scan:row.config,transform:config});
      equal(structured.data,{'a/b':result.text,items:[result.text]});
      equal(api.transformStructured(data,located,config),structured);
    }
  }
  const value='550e8400-e29b-41d4-a716-446655440000';
  const findings=api.scan(value,{detect_uuid:true});
  const config={default:{strategy:'redact'},entities:['UUID']};
  equal(api.scanAndTransform(value,{transform:config}).text,value);
  for(const allow of [{exact:{UUID:[value]}},{regex:{UUID:[{pattern:value}]}}]) equal(api.transform(value,findings,{...config,allow}).text,value);
  equal(api.transform(value,findings,{...config,default:{strategy:'remove'},overrides:{UUID:{strategy:'redact'}},allow:{regex:{UUID:[{pattern:'550e'}]}}}).text,'[UUID]');
  for(const invalid of [null,1,'true',[],{}]) {
    for(const call of [()=>api.scan(value,{detect_uuid:invalid}),()=>api.scanStructured({value},{detect_uuid:invalid})]) {
      let rejected=false;try{call();}catch(error){rejected=error.code==='invalid_configuration'&&error.path==='/detect_uuid';}
      if(!rejected) throw new Error('UUID invalid configuration accepted');
    }
  }
}

export async function verifyUuidProviders(manager,tokenManager,context) {
  const text='👋 550e8400-e29b-41d4-a716-446655440000';
  const makeConfig=strategy=>({scan:{detect_uuid:true},transform:{default:strategy,entities:['UUID']}});
  const pseudonyms=await manager.scanAndTransform(text,makeConfig({strategy:'pseudonymize',key_ref:'uuid'}));
  if(pseudonyms.transformations.length!==1||pseudonyms.text===text) throw new Error('UUID pseudonymization');
  const tokens=await tokenManager.scanAndTransform(text,makeConfig({strategy:'tokenize',token_ref:'uuid'}),context);
  if(tokens.transformations.length!==1||(await tokenManager.restore(tokens.text,context)).text!==text) throw new Error('UUID token round trip');
}
