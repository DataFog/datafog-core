import {execFileSync, spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {createServer} from 'node:http';
import {mkdirSync, mkdtempSync, readFileSync, writeFileSync, existsSync, lstatSync, copyFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {build} from 'esbuild';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const root = path.resolve(packageRoot, '../..');
const args = process.argv.slice(2), settings = {};
for (let i = 0; i < args.length; i += 2) {
  if (!['--core-tarball', '--vault-tarball', '--output', '--engine','--prepared'].includes(args[i]) || !args[i + 1] || settings[args[i]]) throw Error('invalid or duplicate runner argument');
  settings[args[i]] = args[i + 1];
}
for (const key of ['--core-tarball', '--vault-tarball', '--output','--prepared']) if (!settings[key]) throw Error(`required argument: ${key}`);
const engine = settings['--engine'] || 'chromium';
if (!['chromium', 'firefox', 'msedge'].includes(engine)) throw Error('unsupported engine');
const output = path.resolve(settings['--output']);
if (existsSync(output)) throw Error('use a fresh output report path');
mkdirSync(path.dirname(output), {recursive: true});
const work = mkdtempSync(path.join(path.dirname(output), 'browser-consumer-'));
const artifacts = path.join(work, 'artifacts'); mkdirSync(artifacts);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const referenceFiles = ['backup.js','vault.js','storage.js','rpc.js','kdf-worker.js'];
const sourceFiles = ['package.json','package-lock.json','index.mjs','index.d.ts','error.mjs','backup-read.mjs','vault-read.mjs','kdf-worker-source.mjs','scripts/build.mjs','README.md','THIRD_PARTY_NOTICES.md','test/browser.mjs','test/types.ts','test/run-installed.mjs'].map(name=>path.join(packageRoot,name));
for(const file of referenceFiles)sourceFiles.push(path.join(root,'examples/workspace-extension/src',file));
for(const file of ['index.mjs','binding.mjs','index.d.ts','package.json'])sourceFiles.push(path.join(root,'integrations/fogvault-browser',file));
const sourceIdentity=()=>Object.fromEntries(sourceFiles.map(file=>[path.relative(root,file).split(path.sep).join('/'),hash(readFileSync(file))]));
for(const file of ['scripts/verify-fogvault-legacy.py','scripts/fogvault_inputs.py','fixtures/workspace/argon2id.json','fixtures/workspace/argon2id-backup.json'])sourceFiles.push(path.join(root,file));
const before=sourceIdentity();
const required=['legacy_confirmed_save_cancel_cleanup_failure_preserves_request','legacy_fixed_kdf_profiles_match_reference_vectors','legacy_v1_v2_password_recovery_actual_kdf_reopen',
 'legacy_unused_slot_tampering_and_profile_versions_reject',
 'legacy_wrong_credentials_namespace_and_authenticated_head_reject',
 'legacy_literal_approval_opaque_session_and_occupied_destination',
 'legacy_worker_cancel_and_closed_session_never_publish',
 'legacy_archive_admission_and_source_preservation'];
const commands = [], candidates = {}, installed = {};
function run(name, executable, values, cwd = work) {
  const result = spawnSync(executable, values, {cwd, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe']});
  const log = path.join(work, name + '.log'), captured = String(result.stdout || '') + String(result.stderr || '') + String(result.error || '');
  writeFileSync(log, captured);
  commands.push({id: name, command: [executable, ...values], cwd, exit_code: result.status ?? -1, log, log_sha256: hash(Buffer.from(captured))});
  if (result.error || result.status !== 0) throw Error(`${name} failed; see retained command log`);
  return result.stdout;
}
function tarMembers(archive) {
  const names = execFileSync('tar', ['-tzf', archive], {encoding: 'utf8'}).trim().split('\n');
  const lowered = new Set();
  for (const name of names) {
    if (!name.startsWith('package/') || name.includes('\\') || name.split('/').some(x => x === '..' || x === '.')) throw Error('unsafe candidate archive path');
    if (lowered.has(name.toLowerCase())) throw Error('duplicate/colliding candidate member');
    lowered.add(name.toLowerCase());
  }
  const listing = execFileSync('tar', ['-tvzf', archive], {encoding: 'utf8'}).trim().split('\n');
  if (listing.some(line => line[0] !== '-' && line[0] !== 'd')) throw Error('candidate links/special members forbidden');
  return names.filter(name => !name.endsWith('/'));
}
function identity(archive) { return {path: archive, bytes: readFileSync(archive).length, sha256: hash(readFileSync(archive))}; }
let browser, server, result, failure, preparation, environment, typecheck = false;
const externalRequests = [];
const started = performance.now();
try {
  preparation=JSON.parse(run('validate-preparation','python3',[path.join(root,'scripts/verify-fogvault-legacy.py'),'--validate-only','--prepared',path.resolve(settings['--prepared'])],root));
  for(const [key,arg] of [['core_tarball','--core-tarball'],['vault_tarball','--vault-tarball']])if(preparation.artifacts[key].sha256!==hash(readFileSync(settings[arg])))throw Error('supplied candidate differs from prepared proof');
  run('build',process.execPath,[path.join(packageRoot,'scripts/build.mjs')],packageRoot);
  const packed = JSON.parse(run('importer-pack', 'npm', ['pack', '--json', '--ignore-scripts', '--pack-destination', artifacts], packageRoot));
  if (packed.length !== 1 || path.basename(packed[0].filename) !== packed[0].filename) throw Error('invalid adapter package result');
  const specs = [
    ['core', '@datafog/wasm', path.resolve(settings['--core-tarball'])],
    ['vault', '@datafog/fogvault-browser', path.resolve(settings['--vault-tarball'])],
    ['importer', '@datafog/core-fogvault-legacy-browser', path.join(artifacts, packed[0].filename)],
    ['adapter', '@datafog/core-fogvault-browser', path.join(artifacts,JSON.parse(run('adapter-pack','npm',['pack','--json','--ignore-scripts','--pack-destination',artifacts],path.join(root,'integrations/fogvault-browser')))[0].filename)],
  ];
  for (const spec of specs) {
    const [key, , original] = spec;
    const frozen = path.join(artifacts, key + '-input.tgz'); copyFileSync(original, frozen); spec[2] = frozen;
    candidates[key] = {...identity(frozen), source_archive: original}; tarMembers(frozen);
  }
  writeFileSync(path.join(work, 'package.json'), JSON.stringify({name: 'datafog-synthetic-joint-consumer', version: '0.0.0', type: 'module', private: true}));
  run('candidate-install', 'npm', ['install','--offline', '--ignore-scripts', '--no-audit', '--no-fund', ...specs.map(spec => spec[2])]);
  for (const [, name, archive] of specs) {
    installed[name] = {};
    for (const member of tarMembers(archive)) {
      const relative = member.slice('package/'.length), file = path.join(work, 'node_modules', name, relative);
      if (!existsSync(file) || lstatSync(file).isSymbolicLink() || !lstatSync(file).isFile()) throw Error('installed candidate file missing/linked');
      const expected = execFileSync('tar', ['-xOzf', archive, member], {maxBuffer: 64 * 1024 * 1024});
      const actual = readFileSync(file);
      if (hash(actual) !== hash(expected)) throw Error('installed candidate bytes differ from tar');
      installed[name][relative] = hash(actual);
    }
  }
  writeFileSync(path.join(work, 'types.ts'), readFileSync(path.join(packageRoot, 'test/types.ts')));
  run('installed-types', process.execPath, [path.join(packageRoot,'node_modules/typescript/bin/tsc'), '--noEmit', '--strict', '--module', 'nodenext', '--target', 'es2022', '--lib', 'es2022,dom', 'types.ts']);
  typecheck = true;
  copyFileSync(path.join(root,'fixtures/workspace/argon2id.json'),path.join(work,'argon2id.json'));copyFileSync(path.join(root,'fixtures/workspace/argon2id-backup.json'),path.join(work,'argon2id-backup.json'));
  const reference=path.join(work,'reference');mkdirSync(reference);
  for(const file of referenceFiles)if(file!=='kdf-worker.js')copyFileSync(path.join(root,'examples/workspace-extension/src',file),path.join(reference,file));
  await build({entryPoints:[path.join(root,'examples/workspace-extension/src/kdf-worker.js')],nodePaths:[path.join(packageRoot,'node_modules')],bundle:true,platform:'browser',format:'esm',target:'es2022',outfile:path.join(reference,'kdf-worker.js'),legalComments:'eof'});
  writeFileSync(path.join(work, 'browser.mjs'), readFileSync(path.join(packageRoot, 'test/browser.mjs')));
  writeFileSync(path.join(work, 'index.html'), '<!doctype html><meta charset="utf-8"><link rel="icon" href="data:,"><title>Synthetic Core FogVault installed candidate</title><script type="importmap">{"imports":{"@datafog/core-fogvault-browser":"./node_modules/@datafog/core-fogvault-browser/index.mjs"}}</script><script type="module" src="./browser.mjs"></script>');
  server = createServer((request, response) => {
    const pathname = new URL(request.url, 'http://localhost').pathname;
    const file = path.resolve(work, '.' + (pathname === '/' ? '/index.html' : pathname));
    if (!file.startsWith(work + path.sep)) { response.writeHead(403).end(); return; }
    try {
      const contentType = /\.m?js$/.test(file) ? 'text/javascript; charset=utf-8' : file.endsWith('.wasm') ? 'application/wasm' : file.endsWith('.html') ? 'text/html; charset=utf-8' : 'application/octet-stream';
      response.writeHead(200, {'content-type': contentType, 'cache-control': 'no-store','cross-origin-opener-policy':'same-origin','cross-origin-embedder-policy':'require-corp'}); response.end(readFileSync(file));
    } catch { response.writeHead(404).end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const require = createRequire(path.join(packageRoot,'package.json'));
  const playwright = require('playwright');
  browser = await (engine === 'firefox' ? playwright.firefox : playwright.chromium).launch(engine === 'msedge' ? {channel: 'msedge'} : {});
  environment={node:process.version,browserVersion:browser.version(),playwright:JSON.parse(readFileSync(path.join(packageRoot,'node_modules/playwright/package.json'))).version,libsodiumWrappers:JSON.parse(readFileSync(path.join(packageRoot,'node_modules/libsodium-wrappers-sumo/package.json'))).version};
  const origin = `http://127.0.0.1:${server.address().port}`;
  const context = await browser.newContext();
  await context.route('**/*', route => {
    const url = new URL(route.request().url());
    if (url.origin !== origin && !['blob:', 'data:', 'about:'].includes(url.protocol)) {
      externalRequests.push(url.origin); return route.abort();
    }
    return route.continue();
  });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(origin);
  await page.waitForFunction(() => !!window.coreFogVaultDone, null, {timeout:120000});
  result = await page.evaluate(() => window.coreFogVaultDone);
  if (errors.length || externalRequests.length || !result || result.pass !== true || result.executed !== true || result.failure ||
      result.cases.length !== required.length || new Set(result.cases.map(c => c.id)).size !== required.length ||
      required.some(id => !result.cases.some(c => c.id === id && c.status === 'pass' && c.executed === true))) {
    throw Error('required browser integration case failed or was missing: ' + JSON.stringify({errors, result}));
  }
  const combinations=result.cases.find(c=>c.id==='legacy_v1_v2_password_recovery_actual_kdf_reopen')?.combinations;
  const expected=['v1_password','v1_recovery','v2_password','v2_recovery'];
  if(!Array.isArray(combinations)||combinations.length!==4||expected.some(id=>!combinations.some(c=>c.id===id&&c.status==='pass'&&c.executed===true)))throw Error('legacy credential/envelope matrix incomplete');
  if(JSON.stringify(preparation)!==JSON.stringify(JSON.parse(run('validate-preparation-after','python3',[path.join(root,'scripts/verify-fogvault-legacy.py'),'--validate-only','--prepared',path.resolve(settings['--prepared'])],root))))throw Error('prepared source proof changed');
  for(const item of Object.values(candidates))if(hash(readFileSync(item.path))!==item.sha256)throw Error('candidate bytes changed');
  for(const [name,files] of Object.entries(installed))for(const [file,expected] of Object.entries(files))if(hash(readFileSync(path.join(work,'node_modules',name,file)))!==expected)throw Error('installed bytes changed');
  if (JSON.stringify(before) !== JSON.stringify(sourceIdentity())) throw Error('adapter/fixture inputs changed during verification');
} catch (error) { failure = error.message; }
finally {
  await browser?.close();
  if (server) await new Promise(resolve => server.close(resolve));
  const report = {schema: 'datafog-fogvault-legacy-browser-proof/v1', pass: !failure, executed: true, engine, environment,
    run_directory:work, required_cases: required, required_combinations:['v1_password','v1_recovery','v2_password','v2_recovery'], preparation, result, candidates, installed_assets: installed,
    typecheck_executed: typecheck, commands, failure: failure || null, source_identity: before,
    captured_external_requests: externalRequests,
    advisory_duration_ms: performance.now() - started,
    qualification: 'Synthetic installed candidate on the recorded engine; not physical device, shipping extension or security approval.'};
  writeFileSync(output, JSON.stringify(report, null, 2) + '\n', {flag: 'wx'});
  console.log(JSON.stringify({pass: report.pass, report: output, cases: result?.cases?.length || 0}));
  if (failure) process.exitCode = 1;
}
