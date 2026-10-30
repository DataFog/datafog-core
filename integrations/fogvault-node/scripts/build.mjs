import {spawnSync} from 'node:child_process';
import {copyFileSync,mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {nativeTarget,artifactFile,nativeFiles,sha256} from '../native-loader.mjs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const root=fileURLToPath(new URL('../',import.meta.url));
const target=process.env.CARGO_TARGET_DIR??path.resolve(root,'../../target/fogvault-language');
mkdirSync(target,{recursive:true});
const result=spawnSync('cargo',['+1.94.1','build','--release','--manifest-path',path.join(root,'Cargo.toml'),'--locked'],{stdio:'inherit',env:{...process.env,CARGO_TARGET_DIR:target,CARGO_PROFILE_DEV_DEBUG:'0',CARGO_INCREMENTAL:'0'}});
if(result.status!==0)process.exit(result.status??1);
const filename=process.platform==='darwin'?'libdatafog_fogvault_node.dylib':process.platform==='win32'?'datafog_fogvault_node.dll':'libdatafog_fogvault_node.so';
const tag=nativeTarget(),file=artifactFile(tag);
copyFileSync(path.join(target,'release',filename),path.join(root,file));
const sourceFiles=Object.fromEntries(nativeFiles.map(name=>[name,sha256(readFileSync(path.join(root,name)))]));
const revision=spawnSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'});
if(revision.status!==0)throw Error('Cannot record native artifact source revision');
const artifact={target:tag,file,sha256:sha256(readFileSync(path.join(root,file))),sourceFiles,
 revision:revision.stdout.trim(),runnerScope:process.env.DATAFOG_NODE_RUNNER_SCOPE??'local',
 platform:process.platform,arch:process.arch,libc:process.platform==='linux'?'gnu':null};
writeFileSync(path.join(root,'native-artifacts.json'),JSON.stringify({schema:'datafog-fogvault-node-artifacts/v1',version:JSON.parse(readFileSync(path.join(root,'package.json'))).version,artifacts:[artifact]},null,2)+'\n');
