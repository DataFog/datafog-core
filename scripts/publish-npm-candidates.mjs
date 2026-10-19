// Publish retained archives verbatim. Existing versions must match their exact integrity.
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync,readdirSync} from 'node:fs';
import path from 'node:path';
const directory=process.argv[2];
const dryRun=process.argv.includes("--dry-run");
const files=readdirSync(directory).filter(f=>f.endsWith('.tgz')).sort((a,b)=>{
  // Native optional packages must exist before the root package is published.
  return Number(/^datafog-node-\d/.test(a))-Number(/^datafog-node-\d/.test(b))||a.localeCompare(b);
});
for (const file of files) {
  const archive=path.join(directory,file);
  const metadata=JSON.parse(execFileSync('tar',['-xOf',archive,'package/package.json'],{encoding:'utf8'}));
  const name=metadata.name, version=metadata.version;
  if (typeof name!=='string'||typeof version!=='string') throw new Error('invalid qualified archive metadata');
  const integrity='sha512-'+createHash('sha512').update(readFileSync(archive)).digest('base64');
  try {
    const remote=JSON.parse(execFileSync('npm',['view',`${name}@${version}`,'dist.integrity','--json'],{encoding:'utf8',stdio:['ignore','pipe','pipe']}));
    if(remote!==integrity) throw new Error(`Published ${name}@${version} differs from the qualified archive`);
    console.log(`${name}@${version} already published with matching integrity`);
    continue;
  } catch(error) {
    let code;
    try { code=JSON.parse(error.stdout?.toString()||'{}').error?.code; } catch {}
    if(code!=='E404') throw error;
  }
  execFileSync('npm',['publish',archive,'--tag',version.includes('-')?'beta':'latest','--ignore-scripts',...(dryRun?['--dry-run']:[])],{stdio:'inherit'});
}
