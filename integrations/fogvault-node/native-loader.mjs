// Private distribution boundary. No install-time download or source-build fallback.
import {createRequire} from 'node:module';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
export const nativeFiles = ['index.mjs','index.d.ts','native-loader.mjs','package.json','README.md','LICENSE'];
const allowed = new Set(['linux-x64-gnu','darwin-arm64','darwin-x64','win32-x64-msvc']);
const failure = message => {const error = new Error(message);error.code='unsupported_native_target';throw error;};
export function nativeTarget(platform=process.platform, arch=process.arch, header=process.report.getReport().header) {
  if(platform==='linux' && !header.glibcVersionRuntime)failure('Core FogVault Node supports GNU libc on Linux; musl is not qualified.');
  const target=`${platform}-${arch}${platform==='linux'?'-gnu':platform==='win32'?'-msvc':''}`;
  if(!allowed.has(target))failure(`Core FogVault Node has no qualified target for ${target}.`);
  return target;
}
export const artifactFile = target => `datafog-fogvault.${target}.node`;
export const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
export function validateArtifacts(manifest, version) {
  if(manifest?.schema!=='datafog-fogvault-node-artifacts/v1' || manifest.version!==version || !Array.isArray(manifest.artifacts) || !manifest.artifacts.length)throw Error('Invalid Core FogVault native artifact manifest');
  const seen=new Set();
  for(const entry of manifest.artifacts){
    if(!allowed.has(entry.target)||seen.has(entry.target)||entry.file!==artifactFile(entry.target)||!/^[a-f0-9]{64}$/.test(entry.sha256)||!entry.sourceFiles||typeof entry.sourceFiles!=='object')throw Error('Invalid or duplicate Core FogVault native artifact');
    seen.add(entry.target);
  }
  return manifest;
}
export function loadNative() {
  const root=path.dirname(fileURLToPath(import.meta.url));
  const pkg=JSON.parse(readFileSync(path.join(root,'package.json'),'utf8'));
  const manifest=validateArtifacts(JSON.parse(readFileSync(path.join(root,'native-artifacts.json'),'utf8')),pkg.version);
  const target=nativeTarget();
  const entry=manifest.artifacts.find(item=>item.target===target);
  if(!entry)failure(`Core FogVault Node archive does not include ${target}; available: ${manifest.artifacts.map(item=>item.target).join(', ')}.`);
  const file=path.join(root,entry.file);
  if(sha256(readFileSync(file))!==entry.sha256)throw Error('Core FogVault native binary checksum mismatch');
  return createRequire(import.meta.url)(file);
}
