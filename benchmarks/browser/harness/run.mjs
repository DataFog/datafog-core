import {execFileSync, spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {copyFile, mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {tmpdir} from 'node:os';
import {fileURLToPath} from 'node:url';
import {canonicalFixtureDigest, loadScenario} from './scenario.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const harnessDir = path.dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const settings = {};
const allowed = new Set(['--backend', '--scenario', '--output-dir', '--artifact-cache', '--qualification-receipt', '--trial-id', '--run-id', '--profile-id', '--order-block', '--run-kind', '--chrome-executable']);
for (let index = 0; index < args.length; index += 2) {
  const key = args[index], value = args[index + 1];
  if (!allowed.has(key) || !value || settings[key]) throw new Error(`invalid or duplicate argument: ${key || ''}`);
  settings[key] = value;
}
for (const key of ['--backend', '--scenario', '--run-kind']) if (!settings[key]) throw new Error(`required argument: ${key}`);
if (!['sqlite', 'fogvault'].includes(settings['--backend'])) throw new Error('--backend must be sqlite or fogvault');
if (!['pilot', 'final'].includes(settings['--run-kind'])) throw new Error('--run-kind must be pilot or final');
const runKind = settings['--run-kind'];
const backend = settings['--backend'];
const trialId = settings['--trial-id'] || `${Date.now()}-${process.pid}`;
const runId = settings['--run-id'] || `browser-pseudonymization-${runKind}`;
const profileId = settings['--profile-id'] || `${backend}-${trialId}`;
const orderBlock = settings['--order-block'] || 'manual';
const output = path.resolve(settings['--output-dir'] || path.join(root, 'target/browser-pseudonymization', runKind, runId, trialId));
const chromeExecutable = settings['--chrome-executable'] || findRetailChrome();
await mkdir(path.dirname(output), {recursive: true});
await mkdir(output, {recursive: false});
const artifactDir = path.resolve(settings['--artifact-cache'] || path.join(path.dirname(output), 'artifacts'));
await mkdir(artifactDir, {recursive: true});
const workspaceRoot = await mkdtemp(path.join(tmpdir(), `datafog-browser-bench-${trialId}-`));
const profile = path.join(workspaceRoot, 'chrome-profile');
const extension = path.join(workspaceRoot, 'extension');
await mkdir(extension);
const startedAt = new Date().toISOString();
const fixture = await loadScenario(path.resolve(settings['--scenario']));
const actualDigest = canonicalFixtureDigest(fixture);
if (actualDigest !== fixture.fixtureSha256) throw new Error(`fixture digest mismatch: expected ${fixture.fixtureSha256}, computed ${actualDigest}`);
const macProductVersion = process.platform === 'darwin' ? spawnSync('sw_vers', ['-productVersion'], {encoding: 'utf8'}).stdout?.trim() : undefined;
const runtime = {node: process.version, host: os.hostname(), platform: os.platform(), kernelRelease: os.release(), ...(macProductVersion ? {macProductVersion} : {}), arch: os.arch(),
  cpu: os.cpus()[0]?.model || 'unavailable', logicalCpus: os.cpus().length, totalMemoryBytes: os.totalmem(), powerState: 'unavailable'};
const packageSpecs = [
  ['@datafog/wasm', '1.1.0'],
  ['@datafog/core-sqlite-browser', '1.1.0'],
  ['@datafog/fogvault-browser', '0.1.0'],
  ['@datafog/core-fogvault-browser', '1.1.0'],
];
const gateQualificationCases = {
  conflicting_save_rejected_without_overwrite: {
    sqlite: ['stale_complete_storage_revision_cas_rejected'], fogvault: ['complete_head_cas_conflict_closes_stale_owner'],
  },
  lost_save_reply_recovers_without_duplicate_activation: {
    sqlite: ['lost_save_never_grants_handoff_and_reopens_complete_state'], fogvault: ['lost_save_reply_reconciles_without_handoff_or_old_approval'],
  },
  lock_during_save_prevents_handoff: {
    sqlite: ['late_save_never_grants_handoff_and_reopens_complete_state'], fogvault: ['late_confirmed_save_after_lock_never_activates'],
  },
  stale_review_rejected: {
    sqlite: [], fogvault: ['complete_head_cas_conflict_closes_stale_owner'],
  },
};
const sha256 = value => createHash('sha256').update(value).digest('hex');
const command = (executable, values, cwd = root) => execFileSync(executable, values, {cwd, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024});
const candidates = [];
let context, restartResult, firstResult, runnerFailure;
let resolvedDependencyManifest;
const browserProcessSamples = [];
let memoryTimer;
let chromePid;

function findRetailChrome() {
  if (process.platform === 'darwin') return '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  if (process.platform === 'win32') return path.join(process.env.PROGRAMFILES || 'C:\\Program Files', 'Google', 'Chrome', 'Application', 'chrome.exe');
  for (const executable of ['google-chrome', 'google-chrome-stable', 'chrome']) {
    const resolved = spawnSync('which', [executable], {encoding: 'utf8'}).stdout?.trim();
    if (resolved) return resolved;
  }
  throw new Error('retail Google Chrome was not found; supply --chrome-executable');
}

function psSamples() {
  if (!chromePid) return;
  try {
    const rows = execFileSync('ps', ['-Ao', 'pid=,ppid=,rss=,command='], {encoding: 'utf8'}).split('\n');
    const row = rows.map(line => line.match(/^\s*(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/)).filter(Boolean)
      .find(match => match[1] === String(chromePid));
    if (row) browserProcessSamples.push({sampledAtUtc: new Date().toISOString(), pid: Number(row[1]), rssBytes: Number(row[3]) * 1024, process: 'Chrome main browser process', method: 'ps RSS'});
  } catch { /* unavailable on a platform without ps */ }
}

async function startMemorySampling() {
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) {
    try {
      const rows = execFileSync('ps', ['-Ao', 'pid=,ppid=,command='], {encoding: 'utf8'}).split('\n');
      const match = rows.map(line => line.match(/^\s*(\d+)\s+(\d+)\s+(.*)$/)).filter(Boolean)
        .find(item => item[3].includes(`--user-data-dir=${profile}`) && !item[3].includes('--type='));
      if (match) { chromePid = Number(match[1]); break; }
    } catch { break; }
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  if (chromePid) { psSamples(); memoryTimer = setInterval(psSamples, 1000); }
}

async function stagePackages() {
  for (const [name, version] of packageSpecs) {
    const filename = `${name.replace('@', '').replace('/', '-')}-${version}.tgz`;
    let file = path.join(artifactDir, filename);
    try { await readFile(file); }
    catch {
      const result = JSON.parse(command('npm', ['pack', `${name}@${version}`, '--json', '--ignore-scripts', '--pack-destination', artifactDir]));
      if (result.length !== 1 || result[0].name !== name || result[0].version !== version) throw new Error(`registry returned unexpected package identity for ${name}@${version}`);
      file = path.join(artifactDir, result[0].filename);
      if (path.basename(file) !== filename) throw new Error(`unexpected npm archive filename for ${name}@${version}`);
    }
    const bytes = await readFile(file);
    candidates.push({name, version, file, bytes: bytes.byteLength, sha256: sha256(bytes), source: 'npm registry exact version'});
  }
  await writeFile(path.join(extension, 'package.json'), JSON.stringify({name: 'datafog-browser-pseudonymization-benchmark', version: '1.0.0', private: true, type: 'module'}));
  command('npm', ['install', '--ignore-scripts', '--no-audit', '--no-fund', '--package-lock=false', '--no-save', ...candidates.map(candidate => candidate.file)], extension);
  for (const candidate of candidates) {
    const installed = JSON.parse(await readFile(path.join(extension, 'node_modules', candidate.name, 'package.json'), 'utf8'));
    if (installed.name !== candidate.name || installed.version !== candidate.version) throw new Error(`installed identity differs from registry archive: ${candidate.name}`);
  }
  const dependencyTree = JSON.parse(command('npm', ['ls', '--all', '--json', '--omit=dev'], extension));
  const resolvedDependencies = [];
  async function recordDependencies(modulesDirectory) {
    for (const entry of await readdir(modulesDirectory, {withFileTypes: true})) {
      if (entry.name === '.bin') continue;
      const packagePath = path.join(modulesDirectory, entry.name);
      if (entry.name.startsWith('@') && entry.isDirectory()) { await recordDependencies(packagePath); continue; }
      if (!entry.isDirectory()) continue;
      try {
        const packageJson = JSON.parse(await readFile(path.join(packagePath, 'package.json'), 'utf8'));
        resolvedDependencies.push({name: packageJson.name, version: packageJson.version, directorySha256: await directoryDigest(packagePath)});
      } catch (error) { if (error.code !== 'ENOENT') throw error; }
      await recordDependencies(path.join(packagePath, 'node_modules')).catch(error => { if (error.code !== 'ENOENT') throw error; });
    }
  }
  await recordDependencies(path.join(extension, 'node_modules'));
  const dependencyBytes = Buffer.from(JSON.stringify({schema: 'datafog-browser-resolved-dependencies/v1', npmTree: dependencyTree, packages: resolvedDependencies}, null, 2) + '\n');
  const dependencyDigest = sha256(dependencyBytes);
  const dependencyPath = path.join(artifactDir, `resolved-dependencies-${dependencyDigest}.json`);
  resolvedDependencyManifest = {path: path.relative(root, dependencyPath), sha256: dependencyDigest, bytes: dependencyBytes.byteLength};
  try { await readFile(dependencyPath); } catch { await writeFile(dependencyPath, dependencyBytes); }
  const manifest = {manifest_version: 3, name: 'DataFog reversible pseudonymization benchmark', version: '1.0.0',
    background: {service_worker: 'background.mjs', type: 'module'},
    content_security_policy: {extension_pages: "script-src 'self' 'wasm-unsafe-eval'; object-src 'none'; connect-src 'self'; worker-src 'self'"}};
  await writeFile(path.join(extension, 'manifest.json'), JSON.stringify(manifest));
  await writeFile(path.join(extension, 'background.mjs'), 'chrome.runtime.onInstalled.addListener(() => {});');
  await writeFile(path.join(extension, 'index.html'), '<!doctype html><meta charset="utf-8"><title>DataFog synthetic benchmark</title><script type="module" src="flow.mjs"></script>');
  await copyFile(path.join(harnessDir, 'flow.mjs'), path.join(extension, 'flow.mjs'));
}

async function loadQualification() {
  if (!settings['--qualification-receipt']) return undefined;
  const file = path.resolve(root, settings['--qualification-receipt']);
  const evidence = JSON.parse(await readFile(file, 'utf8'));
  if (evidence.schema !== 'datafog-browser-benchmark-qualification/v1' || evidence.status !== 'pass' || evidence.backend !== backend) throw new Error('qualification receipt schema, status, or backend is invalid');
  if (evidence.sourceCommit !== sourceCommit || evidence.sourceDirty !== false) throw new Error('qualification evidence was not produced from this clean frozen source commit');
  const expectedArtifacts = Object.fromEntries(candidates.map(item => [`${item.name}@${item.version}`, item.sha256]));
  const suppliedArtifacts = Object.fromEntries(Object.entries(evidence.packageArtifacts || {}).sort());
  if (JSON.stringify(suppliedArtifacts) !== JSON.stringify(Object.fromEntries(Object.entries(expectedArtifacts).sort()))) {
    throw new Error('qualification package hashes do not exactly match the registry artifacts staged for this trial');
  }
  const suiteReceipts = [];
  const qualifiedGates = new Set();
  for (const suite of evidence.suiteReceipts || []) {
    if (!suite.path || !/^[a-f0-9]{64}$/.test(suite.sha256 || '')) throw new Error('qualification suite receipt reference is malformed');
    const bytes = await readFile(path.resolve(root, suite.path));
    if (sha256(bytes) !== suite.sha256) throw new Error(`qualification suite receipt digest mismatch: ${suite.path}`);
    suiteReceipts.push({path: path.relative(root, path.resolve(root, suite.path)), sha256: suite.sha256});
  }
  if (suiteReceipts.length === 0) throw new Error('qualification evidence has no retained suite receipts');
  for (const [gate, entry] of Object.entries(evidence.gates || {})) {
    if (entry?.status === 'pass' && Array.isArray(entry.cases)) qualifiedGates.add(gate);
  }
  if (evidence.wrongCredentials?.status === 'pass' && Array.isArray(evidence.wrongCredentials.cases) && evidence.wrongCredentials.cases.length > 0) {
    qualifiedGates.add('wrong_credentials_rejected');
  }
  const gates = {};
  for (const [gate, allowedCasesByBackend] of Object.entries(gateQualificationCases)) {
    const entry = evidence.gates?.[gate];
    if (!entry || entry.status !== 'pass' || !Array.isArray(entry.cases) || entry.cases.length === 0 ||
        entry.cases.some(id => !allowedCasesByBackend[backend].includes(id))) continue;
    gates[gate] = {...entry, status: 'pass', qualificationSuiteReceipts: suiteReceipts};
  }
  if (!['conflicting_save_rejected_without_overwrite', 'lost_save_reply_recovers_without_duplicate_activation', 'lock_during_save_prevents_handoff']
    .every(gate => gates[gate]?.status === 'pass')) throw new Error('qualification evidence omitted required package-bound fault gates');
  if (backend === 'sqlite' && !qualifiedGates.has('wrong_credentials_rejected')) throw new Error('qualification evidence omitted wrong-credential rejection');
  return {schema: evidence.schema, sourceCommit: evidence.sourceCommit, packageArtifacts: expectedArtifacts, suiteReceipts, gates,
    wrongCredentials: backend === 'sqlite' ? {status: 'pass', qualificationSuiteReceipts: suiteReceipts} : undefined};
}

async function launchChrome(chromium) {
  context = await chromium.launchPersistentContext(profile, {executablePath: chromeExecutable, headless: true,
    ignoreDefaultArgs: ['--disable-extensions'], args: ['--enable-unsafe-extension-debugging', '--no-first-run', '--no-default-browser-check']});
  await context.setOffline(true);
  context.on('request', request => { if (/^https?:/.test(request.url())) externalRequests.push(request.url()); });
  await startMemorySampling();
  const cdp = await context.browser().newBrowserCDPSession();
  const loaded = await cdp.send('Extensions.loadUnpacked', {path: extension});
  if (!loaded.id) throw new Error('Chrome CDP Extensions.loadUnpacked did not return an extension id');
  const extensionId = loaded.id;
  await cdp.detach();
  const worker = context.serviceWorkers()[0] || await context.waitForEvent('serviceworker', {timeout: 30000});
  if (new URL(worker.url()).host !== extensionId) throw new Error('loaded extension service worker id differs from CDP load result');
  const page = await context.newPage();
  page.on('pageerror', error => pageErrors.push(error.message));
  await page.goto(`chrome-extension://${extensionId}/index.html`);
  await page.waitForFunction(() => globalThis.benchmarkReady === true, null, {timeout: 30000});
  return {page, extensionId};
}

const pageErrors = [], externalRequests = [];
const manifest = {manifest_version: 3};
const sourceCommit = command('git', ['rev-parse', 'HEAD']).trim();
const sourceDirty = command('git', ['status', '--porcelain']).trim();
const receipt = {
  schema: 'datafog-browser-pseudonymization-receipt/v1', schemaVersion: 1,
  runId, runKind, trialId, profileId, orderBlock, backend,
  timestampUtc: startedAt,
  scenarioId: fixture.scenario.scenarioId, scenarioVersion: fixture.scenario.scenarioVersion,
  scenario: fixture.scenario, fixtureSha256: fixture.fixtureSha256,
  repositoryCommit: sourceCommit, harnessCommit: sourceCommit, repositoryDirty: Boolean(sourceDirty), repositoryStatus: sourceDirty,
  runtime, browser: {name: 'Google Chrome retail', executable: chromeExecutable, channel: 'installed retail binary', headless: true},
  packageArtifacts: [], extensionArtifact: null, backendConfiguration: null,
  admission: {status: 'unknown'}, correctness: {gates: {}}, timings: {}, measurements: {},
  externalRequests, pageErrors, exclusionReason: null,
};

try {
  if (runKind === 'final' && sourceDirty) throw new Error('final trials require a clean frozen harness commit');
  await stagePackages();
  await writeFile(path.join(artifactDir, 'package-artifact-manifest.json'), JSON.stringify({schema: 'datafog-browser-package-artifacts/v1', source: 'npm registry exact versions',
    packages: Object.fromEntries(candidates.map(item => [`${item.name}@${item.version}`, {path: path.relative(root, item.file), bytes: item.bytes, sha256: item.sha256}]))}, null, 2) + '\n');
  const qualification = await loadQualification();
  const loadedDirectorySha256 = await directoryDigest(extension);
  const extensionZip = path.join(artifactDir, `mv3-extension-${loadedDirectorySha256}.zip`);
  try { await readFile(extensionZip); }
  catch { command('zip', ['-qr', extensionZip, 'extension'], workspaceRoot); }
  const extensionBytes = await readFile(extensionZip);
  receipt.extensionArtifact = {path: path.relative(root, extensionZip), bytes: extensionBytes.byteLength,
    sha256: sha256(extensionBytes), loadedDirectorySha256};
  receipt.resolvedDependencyManifest = resolvedDependencyManifest;
  const playwrightModule = path.join(root, 'bindings/wasm/node_modules/playwright/index.mjs');
  const {chromium} = await import(playwrightModule);
  const first = await launchChrome(chromium);
  receipt.browser.version = context.browser().version();
  receipt.browser.profileId = profileId;
  receipt.browser.profileLifecycle = 'unique temporary user-data directory per trial; same directory reused across the deliberate full process restart';
  firstResult = await first.page.evaluate(async ({testFixture, metadata}) => {
    try { return await globalThis.runBenchmark(testFixture, metadata); }
    catch (error) { return {...(globalThis.benchmarkDone || {}), failure: {name: error?.name || 'Error', code: error?.code || 'error', message: error?.message || String(error)}}; }
  }, {testFixture: fixture, metadata: {backend, trialId, profileId, runKind}});
  if (!firstResult) throw new Error(`benchmark flow returned no result; page state=${JSON.stringify(await first.page.evaluate(() => ({url: location.href, ready: globalThis.benchmarkReady, flowType: typeof globalThis.runBenchmark, done: globalThis.benchmarkDone})))}`);
  if (firstResult.failure) throw Object.assign(new Error(firstResult.failure.message), {code: firstResult.failure.code});
  await context.close(); context = undefined;
  if (memoryTimer) clearInterval(memoryTimer); memoryTimer = undefined; chromePid = undefined;
  const restarted = await launchChrome(chromium);
  const restartedResult = await restarted.page.evaluate(({id, kind}) => globalThis.runBenchmarkRestart(id, kind), {id: trialId, kind: backend});
  restartResult = restartedResult;
  await context.close(); context = undefined;
  if (memoryTimer) clearInterval(memoryTimer); memoryTimer = undefined;
  receipt.admission = firstResult.admission;
  receipt.correctness = firstResult.correctness;
  for (const [gate, result] of Object.entries(restartResult.correctness?.gates || {})) receipt.correctness.gates[gate] = result;
  if (qualification) {
    for (const [gate, evidence] of Object.entries(qualification.gates)) {
      if (receipt.correctness.gates[gate]?.status === 'unsupported') receipt.correctness.gates[gate] = evidence;
    }
    if (backend === 'sqlite' && receipt.correctness.gates.wrong_credentials_rejected?.status === 'unsupported') {
      receipt.correctness.gates.wrong_credentials_rejected = qualification.wrongCredentials;
    }
    receipt.qualificationEvidence = qualification;
  }
  receipt.timings = {...firstResult.timings, ...(restartResult.timings || {})};
  receipt.measurements = {...firstResult.measurements, browserProcessMemorySamples: browserProcessSamples,
    postRestart: restartResult, pageErrors, externalRequests};
  receipt.backendConfiguration = firstResult.backendConfiguration;
  receipt.exclusionReason = firstResult.exclusionReason || (externalRequests.length ? 'external network request observed' : null);
  if (pageErrors.length || externalRequests.length) throw new Error('page error or unexpected network activity invalidated trial');
  receipt.completedAtUtc = new Date().toISOString();
  receipt.status = 'complete';
} catch (error) {
  runnerFailure = error;
  receipt.status = 'failed';
  receipt.completedAtUtc = new Date().toISOString();
  receipt.failure = {name: error?.name || 'Error', message: error?.message || String(error), stack: error?.stack};
  if (firstResult?.failure) receipt.flowFailure = firstResult.failure;
  receipt.admission = firstResult?.admission || {status: 'unknown'};
  receipt.correctness = firstResult?.correctness || {gates: {harness: {status: 'fail', detail: error.message}}};
  receipt.timings = {...(firstResult?.timings || {}), ...(restartResult?.timings || {})};
  receipt.measurements = {...(firstResult?.measurements || {}), browserProcessMemorySamples: browserProcessSamples, pageErrors, externalRequests};
  receipt.exclusionReason = firstResult?.exclusionReason || 'trial failed; raw failure retained';
} finally {
  if (memoryTimer) clearInterval(memoryTimer);
  await context?.close().catch(() => {});
  receipt.completedAtUtc ||= new Date().toISOString();
  receipt.browserProcessMemory = summarizeMemory(browserProcessSamples);
  receipt.instrumentation = {browserProcessSampling: browserProcessSamples.length ? {status: 'measured', method: 'ps RSS at one second intervals; includes browser process, not child renderer/utility processes'} : {status: 'unavailable', reason: 'Chrome main process could not be identified using the profile path'},
    performanceClock: 'performance.now in extension page', hostClock: 'Node process wall clock', observerOverhead: 'process sampling runs on host at one-second intervals; page timing excludes host-side process sampling'};
  if (runnerFailure) {
    const failureWorkspace = path.join(output, 'failure-workspace');
    await rename(workspaceRoot, failureWorkspace).catch(() => {});
    receipt.failureWorkspace = path.relative(root, failureWorkspace);
  } else {
    await rm(workspaceRoot, {recursive: true, force: true});
    receipt.measurements.temporaryBrowserProfile = {status: 'removed after successful trial', uniquePerTrial: true, reusedAcrossRestart: true};
  }
  receipt.packageArtifacts = candidates.map(item => ({...item, file: path.relative(root, item.file)}));
  await writeFile(path.join(output, 'receipt.json'), JSON.stringify(receipt, null, 2) + '\n');
  await writeFile(path.join(output, 'receipt.jsonl'), JSON.stringify(receipt) + '\n');
  console.log(JSON.stringify({status: receipt.status, receipt: path.join(output, 'receipt.json'), backend, trialId, runKind}));
}
if (runnerFailure) process.exitCode = 1;

async function directoryDigest(directory) {
  const entries = [];
  async function walk(current, prefix = '') {
    for (const entry of (await readdir(current, {withFileTypes: true})).sort((a, b) => a.name.localeCompare(b.name))) {
      const full = path.join(current, entry.name), relative = path.posix.join(prefix, entry.name);
      if (entry.isDirectory()) await walk(full, relative);
      else if (entry.isFile()) { const data = await readFile(full); entries.push({path: relative, bytes: data.byteLength, sha256: sha256(data)}); }
    }
  }
  await walk(directory);
  return sha256(Buffer.from(JSON.stringify(entries)));
}

function summarizeMemory(samples) {
  if (!samples.length) return {status: 'unavailable'};
  const rss = samples.map(sample => sample.rssBytes).sort((a, b) => a - b);
  return {status: 'measured', process: 'Chrome main browser process', sampleCount: rss.length,
    minRssBytes: rss[0], maxRssBytes: rss.at(-1), meanRssBytes: Math.round(rss.reduce((sum, value) => sum + value, 0) / rss.length)};
}
