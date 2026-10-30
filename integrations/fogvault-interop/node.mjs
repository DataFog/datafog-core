import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const [mode,configPath,modulePath]=process.argv.slice(2);
const c=JSON.parse(readFileSync(configPath,'utf8'));
const {NativeWorkspace}=await import(pathToFileURL(modulePath).href);
const id=name=>Uint8Array.from(c[name]);
const options={tenant:c.tenant,protectedControlPath:c.control,vaultPath:c.vault,
  password:Uint8Array.from(c.password),recordId:id('recordId'),namespace:id('namespace'),profile:'password_only'};
const normalize=head=>({revision:String(head.revision),commitId:Array.from(head.commitId)});
const source=c.source;
if(mode==='produce'){
  const owner=NativeWorkspace.createNew({...options,operationId:id('createOperation'),requestId:id('createRequest')});
  try{
    const start=Buffer.byteLength(source.slice(0,source.indexOf(c.value)));
    const plan=owner.prepareReview(c.tenant,[{text:source,scannerFindings:[{
      entityType:'EMAIL',matchedText:c.value,range:{unit:'utf8_bytes',start,end:start+Buffer.byteLength(c.value)},
    }],userSelections:[],corrections:[]}],id('nodeOperation'));
    assert.equal(plan.previews(c.tenant)[0].transformations[0].sourceUtf16Range.start,3);
    const saved=owner.commit(c.tenant,plan,true,id('nodeRequest'));
    assert.equal(owner.restore(c.tenant,saved.previews[0]),source);
    writeFileSync(c.nodeResult,JSON.stringify({head:normalize(saved.head),marker:saved.previews[0]}));
  }finally{owner.close(c.tenant);}
}else if(mode==='verify'){
  const original=JSON.parse(readFileSync(c.nodeResult,'utf8'));
  const py=JSON.parse(readFileSync(c.pythonResult,'utf8'));
  const owner=NativeWorkspace.openExisting({...options,expectedHead:py.head});
  try{
    assert.deepEqual(normalize(owner.head(c.tenant)),py.head);
    assert.equal(owner.restore(c.tenant,original.marker),source);
    assert.equal(owner.restore(c.tenant,py.marker),c.pythonSource);
    assert.equal(owner.restoreInConversation(c.tenant,py.sentMarker,id('logicalId')),c.pythonSource);
    const records=owner.operationsPage(c.tenant,0,100).records;
    assert.ok(records.some(row=>row.occurredAtMs===c.maximumTime-1));
    assert.ok(records.some(row=>row.occurredAtMs===c.maximumTime));
    assert.ok(owner.counts(c.tenant).confirmedHandoffs>=1);
    writeFileSync(c.finalResult,JSON.stringify({head:normalize(owner.head(c.tenant)),
      oldMarkerPreserved:true,pythonMarkerRestored:true,scopedIssuancePreserved:true,
      maximumAdmittedTimesExact:true,counts:owner.counts(c.tenant)}));
  }finally{owner.close(c.tenant);}
}else throw Error('Unknown interop mode');
