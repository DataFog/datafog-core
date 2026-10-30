import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,copyFileSync,rmSync} from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {nativeTarget,nativeFiles,artifactFile,validateArtifacts,sha256} from '../native-loader.mjs';
import {assemble,validateQualified} from '../scripts/assemble.mjs';
const root=fileURLToPath(new URL('../',import.meta.url));
const version=JSON.parse(readFileSync(path.join(root,'package.json'))).version;
const revision='a'.repeat(40);
const sourceFiles=()=>Object.fromEntries(nativeFiles.map(name=>[name,sha256(readFileSync(path.join(root,name)))]));
const names=['roundtrip_approval_lock_reopen_unicode','tenant_foreign_and_hostile_transport_reject','conflict_and_payload_admission_preserve_saved_state','conversation_pending_and_scoped_restore','streams_terminal_transport_and_lifetimes','dropped_ack_reconciliation_requires_fresh_owner','errors_are_structured_and_content_free','authenticated_import_uses_fresh_destination','abandoned_handles_release_native_capacity'];
function pair(scope,target){
 const [platform,arch]=target.split('-'),bytes=Buffer.from('synthetic test bytes: not an executable binary');
 const entry={runnerScope:scope,target,file:artifactFile(target),sha256:sha256(bytes),sourceFiles:sourceFiles(),revision,platform,arch,libc:platform==='linux'?'gnu':null};
 const manifest={schema:'datafog-fogvault-node-artifacts/v1',version,artifacts:[entry]};
 const cases=[...['native_binding_preserves_u64_and_unknown_request_error','native_binding_rejects_wrong_thread_before_owner_access'].map(name=>'tests::'+name),...names.map(name=>'installed_node_'+name)].map(id=>({id,status:'pass',executed:true}));
 const report={schema:'datafog-fogvault-node-installed/v1',pass:true,executed:true,installedTypesVerified:true,source:{commit:revision,dirty:false,inputs:Object.fromEntries(Object.entries(sourceFiles()).map(([n,h])=>['integrations/fogvault-node/'+n,h]))},tools:{platform,arch},nativeArtifact:structuredClone(entry),artifacts:{sourceAddon:{sha256:entry.sha256},installedAddon:{sha256:entry.sha256},packageVersion:version},cases};
 return {manifest,report,bytes};
}
test('native loader selects only qualified GNU/MSVC/macOS targets',()=>{
 assert.equal(nativeTarget('linux','x64',{glibcVersionRuntime:'2.39'}),'linux-x64-gnu');
 assert.equal(nativeTarget('darwin','arm64',{}),'darwin-arm64');
 assert.equal(nativeTarget('win32','x64',{}),'win32-x64-msvc');
 for(const args of [['linux','x64',{}],['linux','arm64',{glibcVersionRuntime:'2.39'}],['win32','arm64',{}]])assert.throws(()=>nativeTarget(...args),error=>error.code==='unsupported_native_target');
});
test('manifest and qualified report reject version, target, duplicate, source and evidence drift',()=>{
 const good=pair('ubuntu-24.04','linux-x64-gnu');
 assert.equal(validateQualified(good.manifest,good.report,revision,sourceFiles(),'ubuntu-24.04').target,'linux-x64-gnu');
 for(const mutate of [x=>x.manifest.version='other',x=>x.manifest.artifacts.push(x.manifest.artifacts[0]),x=>x.manifest.artifacts[0].file='../addon.node',x=>x.report.source.commit='b'.repeat(40),x=>x.report.source.dirty=true,x=>x.report.pass=false,x=>x.report.cases.pop(),x=>x.report.installedTypesVerified=false,x=>x.report.artifacts.installedAddon.sha256='0'.repeat(64),x=>x.manifest.artifacts[0].sourceFiles['index.mjs']='0'.repeat(64),x=>x.manifest.artifacts[0].target='win32-x64-msvc']){
  const value=structuredClone(good);mutate(value);assert.throws(()=>validateQualified(value.manifest,value.report,revision,sourceFiles(),'ubuntu-24.04'));
 }
 assert.throws(()=>validateArtifacts({...good.manifest,artifacts:[]},version));
});
test('loader rejects checksum mismatch before attempting to load native code',async()=>{
 const directory=mkdtempSync(path.join(os.tmpdir(),'dfv-loader-'));
 try{
  copyFileSync(path.join(root,'native-loader.mjs'),path.join(directory,'native-loader.mjs'));
  writeFileSync(path.join(directory,'package.json'),JSON.stringify({version,type:'module'}));
  const target=nativeTarget(),{manifest,bytes}=pair('local',target);
  writeFileSync(path.join(directory,'native-artifacts.json'),JSON.stringify(manifest));
  writeFileSync(path.join(directory,artifactFile(target)),Buffer.concat([bytes,Buffer.from('tampered')]));
  const loader=await import(pathToFileURL(path.join(directory,'native-loader.mjs')));
  assert.throws(()=>loader.loadNative(),/checksum mismatch/);
 }finally{rmSync(directory,{recursive:true,force:true});}
});
test('assembly packs all three observed targets and rejects absent, duplicate or corrupted inputs',()=>{
 const directory=mkdtempSync(path.join(os.tmpdir(),'dfv-assembly-'));
 try{
  const input=path.join(directory,'input');mkdirSync(input);
  for(const [scope,target] of [['ubuntu-24.04','linux-x64-gnu'],['macos-15','darwin-arm64'],['windows-2025','win32-x64-msvc']]){
   const base=path.join(input,scope);mkdirSync(base);const {manifest,report,bytes}=pair(scope,target);
   writeFileSync(path.join(base,'native-artifacts.json'),JSON.stringify(manifest));writeFileSync(path.join(base,'qualification.json'),JSON.stringify(report));writeFileSync(path.join(base,artifactFile(target)),bytes);
  }
  const result=assemble(input,path.join(directory,'success'),revision);
  assert.deepEqual(result.targets,['darwin-arm64','linux-x64-gnu','win32-x64-msvc']);assert.equal(result.published,false);
  const binary=path.join(input,'ubuntu-24.04',artifactFile('linux-x64-gnu'));
  writeFileSync(binary,'corrupt');assert.throws(()=>assemble(input,path.join(directory,'corrupt'),revision),/checksum/);
  const {bytes}=pair('ubuntu-24.04','linux-x64-gnu');writeFileSync(binary,bytes);
  const manifestPath=path.join(input,'macos-15','native-artifacts.json'),original=readFileSync(manifestPath);
  const wrong=pair('ubuntu-24.04','linux-x64-gnu');writeFileSync(manifestPath,JSON.stringify(wrong.manifest));assert.throws(()=>assemble(input,path.join(directory,'duplicate'),revision));writeFileSync(manifestPath,original);
  rmSync(path.join(input,'windows-2025'),{recursive:true});assert.throws(()=>assemble(input,path.join(directory,'missing'),revision),/three/);
 }finally{rmSync(directory,{recursive:true,force:true});}
});
