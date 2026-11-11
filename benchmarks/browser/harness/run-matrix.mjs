import {spawnSync} from 'node:child_process';
import {mkdir, readFile, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const harnessDir = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(harnessDir, '../../..');
const args = process.argv.slice(2), settings = {};
const allowed = new Set(['--catalog', '--trials', '--run-kind', '--run-id', '--output-root', '--chrome-executable', '--sqlite-qualification', '--fogvault-qualification']);
for (let index = 0; index < args.length; index += 2) {
  const key = args[index], value = args[index + 1];
  if (!allowed.has(key) || !value || settings[key]) throw new Error(`invalid or duplicate argument: ${key || ''}`);
  settings[key] = value;
}
for (const key of ['--catalog', '--trials', '--run-kind']) if (!settings[key]) throw new Error(`required argument: ${key}`);
const trials = Number(settings['--trials']);
if (!Number.isSafeInteger(trials) || trials < 1) throw new Error('--trials must be a positive integer');
const runKind = settings['--run-kind'];
if (!['pilot', 'final'].includes(runKind)) throw new Error('--run-kind must be pilot or final');
const runId = settings['--run-id'] || `browser-matrix-${runKind}`;
const outputRoot = path.resolve(settings['--output-root'] || path.join(root, 'target/browser-pseudonymization', runKind, runId));
const artifactCache = path.join(outputRoot, 'artifacts');
const fixtureDir = path.join(outputRoot, 'fixtures');
const trialDir = path.join(outputRoot, 'trials');
await mkdir(fixtureDir, {recursive: true}); await mkdir(trialDir, {recursive: true}); await mkdir(artifactCache, {recursive: true});
const catalog = JSON.parse(await readFile(path.resolve(settings['--catalog']), 'utf8'));
if (!Array.isArray(catalog.scenarios) || catalog.scenarios.length === 0) throw new Error('catalog.scenarios must be a non-empty array');
const rows = [];
const matrixFile = path.join(outputRoot, 'matrix.jsonl');
await writeFile(matrixFile, '');
let ordinal = 0;

// A repeat is a backend pair on one machine. Reverse the first/second order
// every repeat and await each process so competing trials never overlap.
for (const descriptor of catalog.scenarios) {
  for (let repeat = 0; repeat < trials; repeat++) {
    const backendOrder = repeat % 2 === 0 ? ['sqlite', 'fogvault'] : ['fogvault', 'sqlite'];
    const fixturePath = path.join(fixtureDir, `${descriptor.scenarioId}.json`);
    const generated = spawnSync('python3', [path.join(root, 'benchmarks/browser/workloads/generate.py'),
      '--mode', descriptor.mode, '--seed', String(descriptor.seed), '--target', String(descriptor.mappingTarget),
      '--history', descriptor.historyProfile, '--reuse', descriptor.valueReuse, '--shape', descriptor.inputShape, '--output', fixturePath],
    {cwd: root, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024});
    if (generated.error || generated.status !== 0) {
      const row = {ordinal: ordinal++, status: 'fixture_generation_failed', scenarioId: descriptor.scenarioId, repeat, backendOrder,
        failure: String(generated.stderr || generated.error || 'generator exited non-zero')};
      rows.push(row); await writeFile(matrixFile, `${JSON.stringify(row)}\n`, {flag: 'a'}); continue;
    }
    for (const backend of backendOrder) {
      const trialId = `${descriptor.scenarioId}-r${String(repeat + 1).padStart(3, '0')}-${backend}`;
      const outputDir = path.join(trialDir, trialId);
      const commandArgs = [path.join(harnessDir, 'run.mjs'), '--backend', backend, '--scenario', fixturePath,
        '--run-kind', runKind, '--run-id', runId, '--trial-id', trialId, '--profile-id', `profile-${trialId}`,
        '--order-block', `${descriptor.scenarioId}-repeat-${repeat + 1}`, '--output-dir', outputDir, '--artifact-cache', artifactCache];
      if (settings['--chrome-executable']) commandArgs.push('--chrome-executable', settings['--chrome-executable']);
      const qualification = settings[backend === 'sqlite' ? '--sqlite-qualification' : '--fogvault-qualification'];
      if (qualification) commandArgs.push('--qualification-receipt', path.resolve(qualification));
      const child = spawnSync(process.execPath, commandArgs, {cwd: root, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024});
      const row = {ordinal: ordinal++, scenarioId: descriptor.scenarioId, fixture: fixturePath, repeat: repeat + 1, backend,
        orderBlock: `${descriptor.scenarioId}-repeat-${repeat + 1}`, outputDir, status: child.status === 0 ? 'complete' : 'failed',
        stdout: child.stdout || '', stderr: child.stderr || '', exitCode: child.status ?? -1};
      rows.push(row); await writeFile(matrixFile, `${JSON.stringify(row)}\n`, {flag: 'a'});
    }
  }
}
const summary = {schema: 'datafog-browser-pseudonymization-matrix/v1', runId, runKind, createdAtUtc: new Date().toISOString(),
  scenarioCount: catalog.scenarios.length, independentPairsPerScenario: trials, sequential: true, backendOrderAlternates: true,
  statusCounts: rows.reduce((counts, row) => { counts[row.status] = (counts[row.status] || 0) + 1; return counts; }, {}),
  matrixReceipt: matrixFile, artifactCache};
await writeFile(path.join(outputRoot, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
console.log(JSON.stringify(summary));
