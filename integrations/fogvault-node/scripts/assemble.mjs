// Nonpublishing assembly from three actually qualified CI runner scopes.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,readdirSync,mkdirSync,copyFileSync,existsSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {nativeFiles,nativeTarget,validateArtifacts,sha256} from '../native-loader.mjs';
const root=fileURLToPath(new URL('../',import.meta.url));
export const scopes={'ubuntu-24.04':['linux-x64-gnu'],'macos-15':['darwin-arm64','darwin-x64'],'windows-2025':['win32-x64-msvc']};
const read=file=>JSON.parse(readFileSync(file,'utf8'));
const digest=file=>sha256(readFileSync(file));
export function validateQualified(manifest,report,revision,sourceFiles,scope) {
  validateArtifacts(manifest,read(path.join(root,'package.json')).version);
  assert.equal(manifest.artifacts.length,1,'one binary per CI runner');
  const entry=manifest.artifacts[0];
  assert(scopes[scope]?.includes(entry.target),'target does not match declared runner scope');
  assert.equal(entry.runnerScope,scope);
  assert.equal(nativeTarget(entry.platform,entry.arch,{glibcVersionRuntime:entry.libc==='gnu'?'recorded':undefined}),entry.target);
  assert.equal(entry.revision,revision,'artifact source revision mismatch');
  assert.deepEqual(entry.sourceFiles,sourceFiles,'packaged source inputs mismatch');
  assert.equal(report.schema,'datafog-fogvault-node-installed/v1');
  assert.equal(report.pass,true);assert.equal(report.executed,true);
  assert.equal(report.installedTypesVerified,true);
  assert.equal(report.source.commit,revision,'qualified source revision mismatch');
  assert.equal(report.source.dirty,false,'assembly requires clean committed CI sources');
  assert.equal(report.nativeArtifact?.target,entry.target);
  assert.deepEqual(report.nativeArtifact,entry,'installed native manifest differs');
  assert.equal(report.tools.platform,entry.platform);assert.equal(report.tools.arch,entry.arch);
  assert.equal(report.artifacts.sourceAddon.sha256,entry.sha256);
  assert.equal(report.artifacts.installedAddon.sha256,entry.sha256);
  assert.equal(report.artifacts.packageVersion,manifest.version);
  for(const [name,hash] of Object.entries(sourceFiles))assert.equal(report.source.inputs[`integrations/fogvault-node/${name}`],hash,'qualified package source mismatch');
  const required=['native_binding_preserves_u64_and_unknown_request_error','native_binding_rejects_wrong_thread_before_owner_access'];
  const native=report.cases.filter(row=>row.id.startsWith('tests::'));
  assert.deepEqual(native.map(row=>row.id).sort(),required.map(name=>'tests::'+name).sort());
  const consumers=['roundtrip_approval_lock_reopen_unicode','tenant_foreign_and_hostile_transport_reject','conflict_and_payload_admission_preserve_saved_state','conversation_pending_and_scoped_restore','streams_terminal_transport_and_lifetimes','dropped_ack_reconciliation_requires_fresh_owner','errors_are_structured_and_content_free','authenticated_import_uses_fresh_destination','abandoned_handles_release_native_capacity'];
  assert.deepEqual(report.cases.map(row=>row.id).sort(),[...required.map(name=>'tests::'+name),...consumers.map(name=>'installed_node_'+name)].sort());
  assert(report.cases.every(row=>row.status==='pass'&&row.executed===true));
  return entry;
}
export function assemble(input,output,revision) {
  assert.match(revision,/^[a-f0-9]{40}$/);
  assert(!existsSync(output),'assembly output must be fresh');
  const sourceFiles=Object.fromEntries(nativeFiles.map(name=>[name,digest(path.join(root,name))]));
  const directories=readdirSync(input,{withFileTypes:true}).filter(entry=>entry.isDirectory());
  assert.equal(directories.length,3,'exactly three runner artifacts required');
  const seen=new Set(),entries=[],inputs=[];
  for(const directory of directories){
    const base=path.join(input,directory.name),manifest=read(path.join(base,'native-artifacts.json')),report=read(path.join(base,'qualification.json'));
    const scope=manifest.artifacts?.[0]?.runnerScope;
    assert(!seen.has(scope),'duplicate runner scope');seen.add(scope);
    const entry=validateQualified(manifest,report,revision,sourceFiles,scope);
    assert.equal(digest(path.join(base,entry.file)),entry.sha256,'binary checksum mismatch');
    entries.push(entry);inputs.push({directory:base,scope,reportSha256:digest(path.join(base,'qualification.json'))});
  }
  assert.deepEqual([...seen].sort(),Object.keys(scopes).sort(),'missing qualified runner');
  const pkg=path.join(output,'package');mkdirSync(pkg,{recursive:true});
  for(const name of nativeFiles)copyFileSync(path.join(root,name),path.join(pkg,name));
  for(const entry of entries){const inputRow=inputs.find(row=>row.scope===entry.runnerScope);copyFileSync(path.join(inputRow.directory,entry.file),path.join(pkg,entry.file));}
  const manifest={schema:'datafog-fogvault-node-artifacts/v1',version:read(path.join(root,'package.json')).version,artifacts:entries.sort((a,b)=>a.target.localeCompare(b.target))};
  writeFileSync(path.join(pkg,'native-artifacts.json'),JSON.stringify(manifest,null,2)+'\n');
  const packed=JSON.parse(execFileSync('npm',['pack','--ignore-scripts','--json','--pack-destination',path.resolve(output)],{cwd:pkg,encoding:'utf8'}));
  assert.equal(packed.length,1);const tarball=path.join(output,packed[0].filename);
  const result={schema:'datafog-fogvault-node-assembly/v1',pass:true,published:false,revision,targets:entries.map(e=>e.target).sort(),inputs,tarball:{path:tarball,sha256:digest(tarball)}};
  writeFileSync(path.join(output,'assembly.json'),JSON.stringify(result,null,2)+'\n');
  return result;
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 const [mode,...args]=process.argv.slice(2),options={};
 for(let i=0;i<args.length;i+=2){assert(['--input','--output','--revision','--report'].includes(args[i])&&args[i+1]&&!options[args[i]],'invalid assembly argument');options[args[i]]=args[i+1];}
 if(mode==='stage'){
  assert(options['--report']&&options['--output']);const manifest=read(path.join(root,'native-artifacts.json')),report=read(options['--report']);
  assert.equal(report.pass,true);assert.equal(manifest.artifacts.length,1);
  mkdirSync(options['--output'],{recursive:true});
  for(const entry of manifest.artifacts)copyFileSync(path.join(root,entry.file),path.join(options['--output'],entry.file));
  copyFileSync(path.join(root,'native-artifacts.json'),path.join(options['--output'],'native-artifacts.json'));
  copyFileSync(options['--report'],path.join(options['--output'],'qualification.json'));
 }else {assert.equal(mode,'assemble');console.log(JSON.stringify(assemble(options['--input'],options['--output'],options['--revision'])));}
}
