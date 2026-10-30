import {spawnSync} from 'node:child_process';
import {readFileSync,writeFileSync,readdirSync,mkdirSync,mkdtempSync,copyFileSync,existsSync,realpathSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import process from 'node:process';
const packageRoot=fileURLToPath(new URL('../',import.meta.url));
const coreRoot=path.resolve(packageRoot,'../..');
const args=process.argv.slice(2);if(args.length!==2||args[0]!=='--output')throw Error('usage: node test/run-installed.mjs --output FILE');
const output=path.resolve(args[1]);mkdirSync(path.dirname(output),{recursive:true});
const run=mkdtempSync(path.join(path.dirname(output),'node-installed-'));
const sha=file=>createHash('sha256').update(readFileSync(file)).digest('hex');
const commands=[];
function command(id,cmd,argv,options={}){
 const result=spawnSync(cmd,argv,{cwd:options.cwd??packageRoot,env:{...process.env,...options.env},encoding:'utf8',maxBuffer:32*1024*1024});
 const log=path.join(run,`${id}.log`);writeFileSync(log,(result.stdout??'')+(result.stderr??''));commands.push({id,command:[cmd,...argv],cwd:options.cwd??packageRoot,exitCode:result.status,log,sha256:sha(log)});
 if(result.error||result.status!==0)throw Error(`${id} failed; see ${log}`);
 return result.stdout;
}
function sourceIdentity(){
 const result={};
 function scan(dir){for(const entry of readdirSync(dir,{withFileTypes:true})){if(['target','node_modules','.git'].includes(entry.name)||entry.name==='addon.node'||entry.name.endsWith('.tgz'))continue;const file=path.join(dir,entry.name);if(entry.isSymbolicLink())throw Error('source symlink rejected');if(entry.isDirectory())scan(file);else if(entry.isFile())result[path.relative(coreRoot,file)]=sha(file);}}
 for(const dir of ['integrations/fogvault-node','integrations/fogvault-native','integrations/fogvault-migration','crates/core','fixtures/fogvault'])scan(path.join(coreRoot,dir));
 return Object.fromEntries(Object.entries(result).sort(([a],[b])=>a.localeCompare(b)));
}
const sourceBefore=sourceIdentity();
const git=(argv)=>spawnSync('git',argv,{cwd:coreRoot,encoding:'utf8'}).stdout.trim();
const proof={schema:'datafog-fogvault-node-installed/v1',pass:false,executed:true,recordedAtUtc:new Date().toISOString(),source:{commit:git(['rev-parse','HEAD']),ref:git(['branch','--show-current']),dirty:!!git(['status','--porcelain']),inputs:sourceBefore},tools:{node:process.version,platform:process.platform,arch:process.arch},commands,cases:[]};
try{
 command('build',process.execPath,[path.join(packageRoot,'scripts/build.mjs')]);
 const cargoEnv={CARGO_TARGET_DIR:path.join(coreRoot,'target/fogvault-language'),CARGO_PROFILE_TEST_DEBUG:'0',CARGO_PROFILE_DEV_DEBUG:'0',CARGO_INCREMENTAL:'0'};
 const rust=command('rust-boundary-tests','cargo',['+1.94.1','test','--manifest-path',path.join(packageRoot,'Cargo.toml'),'--lib','--locked'],{env:cargoEnv});
 for(const name of ['tests::native_binding_preserves_u64_and_unknown_request_error','tests::native_binding_rejects_wrong_thread_before_owner_access']){if(!rust.includes(`test ${name} ... ok`))throw Error(`missing Rust case ${name}`);proof.cases.push({id:name,status:'pass',executed:true});}
 const nodeDirectory=path.dirname(realpathSync(process.execPath));
 const npmCli=[process.env.npm_execpath,path.join(nodeDirectory,'node_modules/npm/bin/npm-cli.js'),path.resolve(nodeDirectory,'../lib/node_modules/npm/bin/npm-cli.js')].find(p=>p&&existsSync(p));
 if(!npmCli)throw Error('npm CLI not found beside Node; install supported Node/npm');
 proof.tools.rustc=command('rustc-version','rustc',['+1.94.1','--version']).trim();
 proof.tools.cargo=command('cargo-version','cargo',['+1.94.1','--version']).trim();
 proof.tools.npm=command('npm-version',process.execPath,[npmCli,'--version']).trim();
 proof.tools.typescript=command('typescript-version',process.execPath,[path.join(packageRoot,'node_modules/typescript/bin/tsc'),'--version']).trim();
 if(proof.tools.typescript!==`Version ${JSON.parse(readFileSync(path.join(packageRoot,'package.json'))).devDependencies.typescript}`)throw Error('installed TypeScript pin mismatch');
 const fixture=command('authenticated-source-fixture','cargo',['+1.94.1','run','--release','--manifest-path',path.join(packageRoot,'Cargo.toml'),'--example','source-fixture','--locked'],{env:cargoEnv});
 const fixturePath=path.join(run,'authenticated-source.json');writeFileSync(fixturePath,fixture);
 const packed=JSON.parse(command('pack',process.execPath,[npmCli,'pack','--ignore-scripts','--json','--pack-destination',run]));
 if(packed.length!==1)throw Error('expected one package');const tarball=path.join(run,packed[0].filename);
 const consumer=path.join(run,'consumer');mkdirSync(path.join(consumer,'test'),{recursive:true});
 writeFileSync(path.join(consumer,'package.json'),JSON.stringify({name:'synthetic-core-fogvault-node-consumer',private:true,type:'module'}));
 command('install',process.execPath,[npmCli,'install','--ignore-scripts','--no-audit','--no-fund','--offline',tarball],{cwd:consumer});
 for(const name of ['consumer.mjs','worker.mjs','types.ts'])copyFileSync(path.join(packageRoot,'test',name),path.join(consumer,'test',name));
 const installed=path.join(consumer,'node_modules/@datafog/core-fogvault-node');
 proof.artifacts={tarball:{path:tarball,sha256:sha(tarball)},sourceAddon:{path:path.join(packageRoot,'addon.node'),sha256:sha(path.join(packageRoot,'addon.node'))},installedAddon:{path:path.join(installed,'addon.node'),sha256:sha(path.join(installed,'addon.node'))},consumerPath:consumer,packageVersion:JSON.parse(readFileSync(path.join(installed,'package.json'))).version};
 if(proof.artifacts.sourceAddon.sha256!==proof.artifacts.installedAddon.sha256)throw Error('installed addon mismatch');
 for(const name of ['index.mjs','index.d.ts','LICENSE','README.md','package.json'])if(sha(path.join(packageRoot,name))!==sha(path.join(installed,name)))throw Error(`installed file mismatch ${name}`);
 command('typescript',process.execPath,[path.join(packageRoot,'node_modules/typescript/bin/tsc'),'--noEmit','--strict','--module','nodenext','--target','es2022','--lib','es2022,dom',path.join(consumer,'test/types.ts')],{cwd:consumer});
 proof.installedTypesVerified=true;
 const stdout=command('installed-consumer',process.execPath,['--expose-gc','--test','--test-reporter=tap',path.join(consumer,'test/consumer.mjs')],{cwd:consumer,env:{DATAFOG_FOGVAULT_NODE_IMPORT:'@datafog/core-fogvault-node',DATAFOG_NODE_TEST_TMP:path.join(run,'sqlite-fixtures'),DATAFOG_NODE_AUTHENTICATED_FIXTURE:fixturePath}});
 const required=[
  'installed_node_roundtrip_approval_lock_reopen_unicode',
  'installed_node_tenant_foreign_and_hostile_transport_reject',
  'installed_node_conflict_and_payload_admission_preserve_saved_state',
  'installed_node_conversation_pending_and_scoped_restore',
  'installed_node_streams_terminal_transport_and_lifetimes',
  'installed_node_dropped_ack_reconciliation_requires_fresh_owner',
  'installed_node_errors_are_structured_and_content_free',
  'installed_node_authenticated_import_uses_fresh_destination',
  'installed_node_abandoned_handles_release_native_capacity',
 ];
 for(const name of required){const pattern=new RegExp(`^ok [0-9]+ - ${name}$`,'m');if(!pattern.test(stdout))throw Error(`missing installed case ${name}`);proof.cases.push({id:name,status:'pass',executed:true});}
 if(!stdout.includes(`# pass ${required.length}`)||!stdout.includes('# fail 0')||!stdout.includes('# skipped 0')||!stdout.includes('# cancelled 0'))throw Error('incomplete consumer results');
 const after=sourceIdentity();if(JSON.stringify(sourceBefore)!==JSON.stringify(after))throw Error('source inputs changed during verification');
 proof.pass=true;
}catch(error){proof.error=error.message;}
writeFileSync(path.join(run,'report.json'),JSON.stringify(proof,null,2)+'\n');
writeFileSync(output,JSON.stringify(proof,null,2)+'\n');
console.log(JSON.stringify({pass:proof.pass,report:output,cases:proof.cases.length}));
if(!proof.pass)process.exitCode=1;
