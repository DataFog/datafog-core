import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFile, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const settings = {}, args = process.argv.slice(2);
const allowed = new Set(['--backend', '--suite-receipt', '--package-manifest', '--output']);
for (let index = 0; index < args.length; index += 2) {
  const key = args[index], value = args[index + 1];
  if (!allowed.has(key) || !value || settings[key]) throw new Error(`invalid or duplicate argument: ${key || ''}`);
  settings[key] = value;
}
for (const key of ['--backend', '--suite-receipt', '--package-manifest', '--output']) if (!settings[key]) throw new Error(`required argument: ${key}`);
const backend = settings['--backend'];
if (!['sqlite', 'fogvault'].includes(backend)) throw new Error('--backend must be sqlite or fogvault');
const hash = value => createHash('sha256').update(value).digest('hex');
const sourceCommit = execFileSync('git', ['rev-parse', 'HEAD'], {cwd: root, encoding: 'utf8'}).trim();
const dirty = execFileSync('git', ['status', '--porcelain'], {cwd: root, encoding: 'utf8'}).trim();
if (dirty) throw new Error('qualification evidence requires a clean source commit');
const suitePath = path.resolve(settings['--suite-receipt']);
const suiteBytes = await readFile(suitePath), suite = JSON.parse(suiteBytes.toString('utf8'));
const packageManifestPath = path.resolve(settings['--package-manifest']);
const packageManifest = JSON.parse(await readFile(packageManifestPath, 'utf8'));
if (packageManifest.schema !== 'datafog-browser-package-artifacts/v1') throw new Error('invalid package artifact manifest');

function run(command, args) { return execFileSync(command, args, {cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024}); }
function localArchiveFiles(archive) {
  const members = run('tar', ['-tzf', archive]).trim().split('\n').filter(name => name && !name.endsWith('/')).sort();
  const result = {};
  for (const member of members) {
    if (!member.startsWith('package/') || member.split('/').includes('..')) throw new Error(`unsafe npm archive member ${member}`);
    result[member.slice('package/'.length)] = hash(execFileSync('tar', ['-xOzf', archive, member], {cwd: root, maxBuffer: 64 * 1024 * 1024}));
  }
  return result;
}
function samePackageContents(registryPath, localPath, name) {
  const expected = localArchiveFiles(registryPath), actual = localArchiveFiles(localPath);
  if (JSON.stringify(expected) !== JSON.stringify(actual)) throw new Error(`${name} installed proof package contents do not match the exact registry artifact`);
  return hash(Buffer.from(JSON.stringify(expected)));
}

const suiteCandidates = backend === 'sqlite' ? {
  '@datafog/wasm@1.1.0': suite.candidates?.core?.file,
  '@datafog/core-sqlite-browser@1.1.0': suite.candidates?.sqlite?.file,
} : {
  '@datafog/core-fogvault-browser@1.1.0': suite.candidates?.adapter?.path,
};
if (backend === 'sqlite') {
  if (suite.schema !== 'datafog-sqlite-browser-proof/v1' || suite.passed !== true || suite.revision !== sourceCommit || suite.git_status !== '') throw new Error('SQLite conformance receipt is incomplete or from another commit');
} else {
  if (suite.schema !== 'datafog-fogvault-browser-proof/v1' || suite.pass !== true || suite.executed !== true || suite.failure) throw new Error('FogVault conformance receipt did not pass');
  for (const [file, digest] of Object.entries(suite.source_identity || {})) {
    if (hash(await readFile(path.join(root, file))) !== digest) throw new Error(`FogVault conformance source changed: ${file}`);
  }
  if (!suite.candidates?.core || !suite.candidates?.vault || suite.candidates.core.sha256 !== packageManifest.packages['@datafog/wasm@1.1.0']?.sha256 ||
      suite.candidates.vault.sha256 !== packageManifest.packages['@datafog/fogvault-browser@0.1.0']?.sha256) throw new Error('FogVault proof did not install the exact Core/FogVault registry artifacts');
}

const contentChecks = {};
for (const [name, localPath] of Object.entries(suiteCandidates)) {
  if (!localPath) throw new Error(`conformance receipt omitted its packaged ${name} candidate`);
  const published = packageManifest.packages[name];
  if (!published) throw new Error(`published artifact manifest omitted ${name}`);
  contentChecks[name] = {sourceArchive: path.resolve(localPath), contentSha256: samePackageContents(published.path, path.resolve(localPath), name)};
}
let observedCases;
if (backend === 'sqlite') observedCases = new Set(suite.cases || []);
else observedCases = new Set((suite.result?.cases || []).filter(item => item.status === 'pass' && item.executed === true).map(item => item.id));
const requiredCases = backend === 'sqlite' ? {
  conflicting_save_rejected_without_overwrite: ['stale_complete_storage_revision_cas_rejected'],
  lost_save_reply_recovers_without_duplicate_activation: ['lost_save_never_grants_handoff_and_reopens_complete_state'],
  lock_during_save_prevents_handoff: ['late_save_never_grants_handoff_and_reopens_complete_state'],
} : {
  conflicting_save_rejected_without_overwrite: ['complete_head_cas_conflict_closes_stale_owner'],
  lost_save_reply_recovers_without_duplicate_activation: ['lost_save_reply_reconciles_without_handoff_or_old_approval'],
  lock_during_save_prevents_handoff: ['late_confirmed_save_after_lock_never_activates'],
};
const gates = {};
for (const [gate, cases] of Object.entries(requiredCases)) {
  const passedCases = cases.filter(id => observedCases.has(id));
  if (passedCases.length === cases.length) gates[gate] = {status: 'pass', cases: passedCases};
}
if (Object.keys(gates).length !== Object.keys(requiredCases).length) throw new Error('conformance receipt omitted one or more required fault cases');
const output = path.resolve(settings['--output']);
const qualification = {
  schema: 'datafog-browser-benchmark-qualification/v1', status: 'pass', backend,
  qualifiedAtUtc: new Date().toISOString(), sourceCommit, sourceDirty: false,
  packageArtifacts: Object.fromEntries(Object.entries(packageManifest.packages).map(([name, item]) => [name, item.sha256])),
  suiteReceipts: [{path: suitePath, sha256: hash(suiteBytes), schema: suite.schema}],
  suiteSourceIdentity: contentChecks, gates,
  engine: backend === 'sqlite' ? 'chromium from playwright integration runner' : suite.engine,
  limitations: 'Adversarial correctness suite is separate from timed trials; its exact package-content identity is proven against the registry artifact. Browser-engine coverage follows the retained suite receipt.',
};
await writeFile(output, JSON.stringify(qualification, null, 2) + '\n', {flag: 'wx'});
console.log(JSON.stringify({status: qualification.status, backend, output, gateCount: Object.keys(gates).length}));
