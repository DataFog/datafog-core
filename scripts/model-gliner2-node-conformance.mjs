// Compare the installed Node package with actual installed Python/Core qualification output.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const [configPath, reportPath, packagePath] = process.argv.slice(2);
const config=JSON.parse(readFileSync(configPath));
const report=JSON.parse(readFileSync(reportPath));
const api=await import(pathToFileURL(packagePath).href);
const session=await api.ModelSession.open(config);
try {
  assert.equal(session.inspect().configuration_id,report.session.configuration_id);
  for (const fixture of report.cases) {
    const result=await session.scan(fixture.text,'model_only');
    assert.equal(result.contractVersion,1);
    assert.equal(result.configurationId,report.session.configuration_id);
    assert.equal(result.mode,'model_only');
    assert.deepEqual(result.evidence,fixture.evidence);
    assert.deepEqual(result.findings.map(f=>({entity:f.entityType,text:f.matchedText,
      start:f.codepointRange.start,end:f.codepointRange.end,byte_start:f.byteRange.start,byte_end:f.byteRange.end})),fixture.findings);
    for (const f of result.findings) {
      assert.equal(fixture.text.slice(f.utf16Range.start,f.utf16Range.end),f.matchedText);
      assert.equal(f.confidence,undefined);
    }
  }
  const text=report.cases[0].text;
  const result=await session.scan(text,'combined');
  assert.notEqual(api.transform(text,result.findings,{default:{strategy:'redact'}}).text,text);
  const data={first_name:'John',note:text,keep:12};
  const structured=await session.scanStructured(data,'combined');
  assert(structured.findings.some(f=>f.path==='/note'));
  assert.equal(structured.evidence.length,structured.findings.length);
  assert.equal(api.transformStructured(data,structured.findings,{default:{strategy:'redact'}}).data.keep,12);
  const cancellation=new api.ModelCancellation();cancellation.cancel();
  await assert.rejects(session.scan(text,'model_only',{cancellation}),e=>e.code==='model_cancelled');
  const pending=session.scan(text,'model_only');
  await assert.rejects(session.scan(text,'model_only'),e=>e.code==='model_busy');
  await pending;
} finally { await session.close(); }
console.log('Installed Node/Python GLiNER2 parity, Unicode, structured transformations, cancellation and cleanup passed.');
