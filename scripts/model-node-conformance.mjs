// Compare installed Node and Python results from the same actual model.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const [configPath, reportPath, packagePath] = process.argv.slice(2);
const config=JSON.parse(readFileSync(configPath));
const report=JSON.parse(readFileSync(reportPath));
const api=await import(pathToFileURL(packagePath).href);
const session=await api.ModelSession.open(config);
try {
  for (const fixture of report.cases) {
    const result=await session.scan(fixture.text);
    assert.equal(result.contractVersion,1);
    assert.equal(result.model,fixture.model);
    assert.deepEqual(result.evidence,fixture.evidence);
    assert.deepEqual(result.findings.map(f=>({entity:f.entityType,text:f.matchedText,
      start:f.codepointRange.start,end:f.codepointRange.end,byte_start:f.byteRange.start,byte_end:f.byteRange.end})),fixture.findings);
    for (const f of result.findings) {
      assert.equal(fixture.text.slice(f.utf16Range.start,f.utf16Range.end),f.matchedText);
      assert.equal(Buffer.from(fixture.text).subarray(f.byteRange.start,f.byteRange.end).toString(),f.matchedText);
      assert.equal([...fixture.text].slice(f.codepointRange.start,f.codepointRange.end).join(''),f.matchedText);
      if (config.gliner2) assert.equal(f.confidence,undefined);
    }
  }
  const cancellation=new api.ModelCancellation(); cancellation.cancel();
  await assert.rejects(session.scan('private@example.test',{cancellation}),e=>e.code==='model_cancelled');
  const pending=session.scan(report.cases[0].text);
  await assert.rejects(session.scan('second'),e=>e.code==='model_busy');
  await pending;
} finally { await session.close(); }
await session.close();
await assert.rejects(session.scan(''),e=>e.code==='model_session_closed');
await assert.rejects(api.ModelSession.open({...config,manifest_sha256:'0'.repeat(64)}),e=>e.code==='model_load_failed');
console.log('Installed Node/Python model results, Unicode, errors and cleanup passed.');
