import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {cpSync, mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const settings = {};
const runnerArgs = process.argv.slice(2);
for (let index = 0; index < runnerArgs.length; index += 2) {
  const key = runnerArgs[index], value = runnerArgs[index + 1];
  if (!['--core-tarball', '--sqlite-tarball'].includes(key) || !value || settings[key]) throw Error(`invalid or duplicate runner argument: ${key || ''}`);
  settings[key] = path.resolve(value);
}
const output = path.join(root, 'target/sqlite-browser', new Date().toISOString().replaceAll(':', '-'));
mkdirSync(output, {recursive: true});
const run = (cmd, args, cwd = root) => execFileSync(cmd, args, {cwd, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024});
const hash = value => createHash('sha256').update(value).digest('hex');
const lock = JSON.parse(readFileSync(path.join(root, 'integrations/sqlite-browser/sqlite3mc-lock.json')));
for (const [name, digest] of Object.entries(lock.files)) {
  if (hash(readFileSync(path.join(root, 'integrations/sqlite-browser/vendor', name))) !== digest) throw Error('unprovisioned/mismatched SQLite3MC asset');
}
const required = [
  'failed_credential_write_closes_owner_and_preserves_existing_credentials', 'invalid_options_do_not_transfer_or_strand_vault_ownership',
  'forget_persists_retirement_revokes_streams_and_does_not_reissue_codes', 'browser_process_restart_preserves_expired_marker',
  'conversation_pending_save_survives_encrypted_reopen_without_scoped_restore',
  'conversation_pending_survives_process_restart_until_approved_observation_save',
  'conversation_saved_observation_and_scoped_restore_survive_process_restart',
  'raw_provider_mutation_rejected_after_transfer', 'approved_save_before_activation_and_exact_restore',
  'lock_revokes_owner_and_wrong_password_rejected', 'password_rotation_and_independent_recovery',
  'authenticated_encrypted_backup_fresh_destination_recovery_and_tamper_rejection', 'encrypted_opfs_close_and_reopen',
  'lost_save_never_grants_handoff_and_reopens_complete_state', 'quota_save_never_grants_handoff_and_reopens_complete_state',
  'late_save_never_grants_handoff_and_reopens_complete_state', 'crash_save_never_grants_handoff_and_reopens_complete_state', 'interrupted_bootstrap_resumes_only_after_credential_authentication',
  'competing_opfs_worker_rejected_without_mutation', 'stale_complete_storage_revision_cas_rejected',
  'persistent_artifacts_contain_no_fixture_plaintext_and_page_tamper_rejects', 'browser_process_restart_preserves_exact_restore',
];
const candidates = {};
for (const [name, directory, setting] of [
  ['core', 'bindings/wasm', '--core-tarball'], ['sqlite', 'integrations/sqlite-browser', '--sqlite-tarball'],
]) {
  if (settings[setting]) candidates[name] = settings[setting];
  else {
    const packed = JSON.parse(run('npm', ['pack', '--json', '--ignore-scripts', '--pack-destination', output], path.join(root, directory)))[0];
    candidates[name] = path.join(output, packed.filename);
  }
}
const extension = path.join(output, 'extension'); mkdirSync(extension);
writeFileSync(path.join(extension, 'package.json'), '{"private":true,"type":"module"}');
run('npm', ['install', '--ignore-scripts', '--no-audit', '--no-fund', candidates.core, candidates.sqlite], extension);
cpSync(path.join(root, 'integrations/sqlite-browser/test/types.ts'), path.join(extension, 'types.ts'));
run(process.execPath, [path.join(root, 'bindings/wasm/node_modules/typescript/bin/tsc'), '--noEmit', '--strict', '--module', 'nodenext', '--target', 'es2022', '--lib', 'es2022,dom', 'types.ts'], extension);
cpSync(path.join(extension, 'node_modules/@datafog'), path.join(extension, 'sdk'), {recursive: true});
writeFileSync(path.join(extension, 'manifest.json'), JSON.stringify({manifest_version: 3, name: 'DataFog SQLite qualification', version: '1.0', background: {service_worker: 'background.js'}, content_security_policy: {extension_pages: "script-src 'self' 'wasm-unsafe-eval'; object-src 'none'; connect-src 'self'; worker-src 'self'"}}));
writeFileSync(path.join(extension, 'background.js'), 'chrome.runtime.onInstalled.addListener(()=>{});');
writeFileSync(path.join(extension, 'index.html'), '<!doctype html><title>SQLite qualification</title><script type="module" src="browser.mjs"></script>');
cpSync(path.join(root, 'integrations/sqlite-browser/test/browser.mjs'), path.join(extension, 'browser.mjs'));
cpSync(path.join(root, 'integrations/sqlite-browser/test/fault-worker.mjs'), path.join(extension, 'fault-worker.mjs'));
const {chromium} = await import(path.join(root, 'bindings/wasm/node_modules/playwright/index.mjs'));
const profile = path.join(output, 'profile');
const requests = [];
const launch = async () => {
  const browser = await chromium.launchPersistentContext(profile, {channel: 'chromium', headless: true, args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]});
  await browser.route(/^https?:/, route => { requests.push(route.request().url()); return route.abort(); });
  return browser;
};
const receipt = {schema: 'datafog-sqlite-browser-proof/v1', revision: run('git', ['rev-parse', 'HEAD']).trim(), git_status: run('git', ['status', '--porcelain', '--untracked-files=no']).trim(), engine: 'Playwright Chromium', candidates: Object.fromEntries(Object.entries(candidates).map(([name, file]) => [name, {file, bytes: readFileSync(file).length, sha256: hash(readFileSync(file))}])), sqlite: lock, cases: []};
let context;
try {
  context = await launch();
  receipt.browserVersion = context.browser().version();
  const worker = context.serviceWorkers()[0] || await context.waitForEvent('serviceworker');
  const id = new URL(worker.url()).host;
  let page = await context.newPage();
  page.on('console', message => { if (process.env.DATAFOG_SQLITE_DEBUG) console.log(message.text()); });
  page.on('pageerror', error => console.error(error.message));
  await page.goto(`chrome-extension://${id}/index.html`);
  await page.waitForFunction(() => globalThis.ready === true);
  receipt.cases.push(...await page.evaluate(() => globalThis.runFirst()));
  receipt.cases.push(...await page.evaluate(() => globalThis.runForget()));
  receipt.cases.push(...await page.evaluate(() => globalThis.runConversation()));
  receipt.cases.push(...await page.evaluate(() => globalThis.runFaults()));
  await context.close(); context = await launch();
  page = await context.newPage();
  await page.goto(`chrome-extension://${id}/index.html`);
  await page.waitForFunction(() => globalThis.ready === true);
  receipt.cases.push(...await page.evaluate(() => globalThis.runRestart()));
  await context.close(); context = await launch();
  page = await context.newPage();
  await page.goto(`chrome-extension://${id}/index.html`);
  await page.waitForFunction(() => globalThis.ready === true);
  receipt.cases.push(...await page.evaluate(() => globalThis.runObservedRestart()));
  if (requests.length) throw Error('external network request');
  if (new Set(receipt.cases).size !== required.length || required.some(name => !receipt.cases.includes(name))) throw Error('missing or duplicate required browser case');
  receipt.required = required; receipt.externalRequests = requests; receipt.typecheck = true;
  receipt.passed = true;
} catch (error) { receipt.passed = false; receipt.failure = error.message; throw error; }
finally { await context?.close(); writeFileSync(path.join(output, 'receipt.json'), JSON.stringify(receipt, null, 2)); console.log(path.join(output, 'receipt.json')); }
