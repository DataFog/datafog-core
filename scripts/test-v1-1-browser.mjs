import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {copyFileSync, mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
if (args.length && (args.length !== 2 || args[0] !== '--registry-version' || !/^\d+\.\d+\.\d+(?:-rc\.\d+)?$/.test(args[1]))) {
  throw Error('Usage: node scripts/test-v1-1-browser.mjs [--registry-version <exact-version>]');
}
const registryVersion = args[1];
const output = path.join(root, 'target/v1-1-browser', new Date().toISOString().replaceAll(':', '-'));
mkdirSync(output, {recursive: true});
const run = (cmd, args, cwd = root) => execFileSync(cmd, args, {cwd, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024});
const hash = file => createHash('sha256').update(readFileSync(file)).digest('hex');
const candidates = [];
for (const directory of ['bindings/wasm', 'integrations/sqlite-browser', 'integrations/fogvault-browser']) {
  const metadata = JSON.parse(readFileSync(path.join(root, directory, 'package.json')));
  const spec = registryVersion ? [`${metadata.name}@${registryVersion}`] : [];
  const packed = JSON.parse(run('npm', ['pack', ...spec, '--json', '--ignore-scripts', '--pack-destination', output], registryVersion ? root : path.join(root, directory)))[0];
  const file = path.join(output, packed.filename);
  candidates.push({file, name: metadata.name, version: registryVersion || metadata.version, bytes: readFileSync(file).length, sha256: hash(file)});
}
const extension = path.join(output, 'extension'); mkdirSync(extension);
writeFileSync(path.join(extension, 'package.json'), '{"private":true,"type":"module"}');
run('npm', ['install', '--ignore-scripts', '--no-audit', '--no-fund', ...candidates.map(item => item.file), '@datafog/fogvault-browser@0.1.0'], extension);
for (const candidate of candidates) {
  const installed = JSON.parse(readFileSync(path.join(extension, 'node_modules', candidate.name, 'package.json')));
  if (installed.name !== candidate.name || installed.version !== candidate.version) throw Error('Installed package identity differs from selected candidate');
}
copyFileSync(path.join(root, 'scripts/v1-1-browser-types.ts'), path.join(extension, 'types.ts'));
run(process.execPath, [path.join(root, 'bindings/wasm/node_modules/typescript/bin/tsc'), '--noEmit', '--strict', '--module', 'nodenext', '--target', 'es2022', '--lib', 'es2022,dom', 'types.ts'], extension);
const manifest = {manifest_version: 3, name: 'DataFog v1.1 joint qualification', version: '1.0', background: {service_worker: 'background.js'}, content_security_policy: {extension_pages: "script-src 'self' 'wasm-unsafe-eval'; object-src 'none'; connect-src 'self'; worker-src 'self'"}};
writeFileSync(path.join(extension, 'manifest.json'), JSON.stringify(manifest));
writeFileSync(path.join(extension, 'background.js'), 'chrome.runtime.onInstalled.addListener(()=>{});');
writeFileSync(path.join(extension, 'index.html'), '<!doctype html><title>v1.1 qualification</title><script type="module" src="browser.mjs"></script>');
copyFileSync(path.join(root, 'scripts/v1-1-browser-fixture.mjs'), path.join(extension, 'browser.mjs'));
const {chromium} = await import(path.join(root, 'bindings/wasm/node_modules/playwright/index.mjs'));
const profile = path.join(output, 'profile'), externalRequests = [];
const launch = async () => {
  const context = await chromium.launchPersistentContext(profile, {channel: 'chromium', headless: true, args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]});
  context.on('request', request => {if (/^https?:/.test(request.url())) externalRequests.push(request.url());});
  await context.setOffline(true); return context;
};
const receipt = {schema: 'datafog-browser-v1-1-proof/v1', typecheck: true, git_status: run('git', ['status', '--porcelain', '--untracked-files=no']).trim(), revision: run('git', ['rev-parse', 'HEAD']).trim(), commit: run('git', ['rev-parse', 'HEAD']).trim(), candidates, manifest, cases: [], externalRequests};
receipt.distribution = registryVersion ? {source: 'npm-registry', version: registryVersion} : {source: 'checkout'};
let context;
try {
  context = await launch(); receipt.browser = context.browser().version();
  const worker = context.serviceWorkers()[0] || await context.waitForEvent('serviceworker');
  const id = new URL(worker.url()).host;
  let page = await context.newPage();
  page.on('pageerror', error => console.error(error.message));
  await page.goto(`chrome-extension://${id}/index.html`);
  await page.waitForFunction(() => globalThis.jointReady === true);
  const first = await page.evaluate(() => globalThis.runJoint());
  receipt.cases.push(...first.cases); receipt.measurements = first.measurements;
  await context.close(); context = await launch();
  page = await context.newPage();
  await page.goto(`chrome-extension://${id}/index.html`);
  await page.waitForFunction(() => globalThis.jointReady === true);
  receipt.cases.push(...await page.evaluate(() => globalThis.runJointRestart()));
  const required = ['sqlite', 'fogvault'].flatMap(kind => [
    `${kind}_default_cnn_review_toggle_save_restore`,
    `${kind}_lock_during_inference_rejects_late_review`,
    `${kind}_process_restart_restore_with_cnn_disabled`,
    `${kind}_cnn_conversation_pending_observed_scoped_restore`,
  ]);
  required.push('fogvault_to_sqlite_approved_fresh_recovery');
  if (receipt.cases.length !== required.length || required.some(name => !receipt.cases.includes(name)) || externalRequests.length) {
    throw Error('incomplete cases or unexpected external request');
  }
  receipt.required = required; receipt.passed = true;
} catch (error) {receipt.passed = false; receipt.failure = error.message; throw error;}
finally {await context?.close(); writeFileSync(path.join(output, 'receipt.json'), JSON.stringify(receipt, null, 2)); console.log(path.join(output, 'receipt.json'));}
