import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {copyFileSync, mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = path.join(root, 'target/v1-1-browser', new Date().toISOString().replaceAll(':', '-'));
mkdirSync(output, {recursive: true});
const run = (cmd, args, cwd = root) => execFileSync(cmd, args, {cwd, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024});
const hash = file => createHash('sha256').update(readFileSync(file)).digest('hex');
const candidates = [];
for (const directory of ['bindings/wasm', 'integrations/sqlite-browser', 'integrations/fogvault-browser']) {
  const packed = JSON.parse(run('npm', ['pack', '--json', '--ignore-scripts', '--pack-destination', output], path.join(root, directory)))[0];
  const file = path.join(output, packed.filename); candidates.push({file, sha256: hash(file)});
}
const extension = path.join(output, 'extension'); mkdirSync(extension);
writeFileSync(path.join(extension, 'package.json'), '{"private":true,"type":"module"}');
run('npm', ['install', '--ignore-scripts', '--no-audit', '--no-fund', ...candidates.map(item => item.file), '@datafog/fogvault-browser@0.1.0'], extension);
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
const receipt = {commit: run('git', ['rev-parse', 'HEAD']).trim(), candidates, manifest, cases: [], externalRequests};
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
  if (receipt.cases.length !== 4 || externalRequests.length) throw Error('incomplete cases or unexpected external request');
  receipt.passed = true;
} catch (error) {receipt.passed = false; receipt.failure = error.message; throw error;}
finally {await context?.close(); writeFileSync(path.join(output, 'receipt.json'), JSON.stringify(receipt, null, 2)); console.log(path.join(output, 'receipt.json'));}
