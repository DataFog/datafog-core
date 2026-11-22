import {createHash} from 'node:crypto';
import {readFile} from 'node:fs/promises';

export const SCENARIO_KEYS = [
  'scenarioVersion', 'scenarioId', 'mode', 'seed', 'mappingTarget',
  'historyProfile', 'valueReuse', 'inputShape', 'fixtureRef',
];

export async function loadScenario(file) {
  const bytes = await readFile(file);
  const fixture = JSON.parse(bytes.toString('utf8'));
  validateScenario(fixture);
  return fixture;
}

export function validateScenario(fixture) {
  if (!fixture || typeof fixture !== 'object' || Array.isArray(fixture)) throw new TypeError('scenario fixture must be a JSON object');
  const scenario = fixture.scenario;
  if (!scenario || typeof scenario !== 'object' || Array.isArray(scenario)) throw new TypeError('fixture.scenario is required');
  for (const key of SCENARIO_KEYS) if (!(key in scenario)) throw new TypeError(`scenario.${key} is required`);
  if (scenario.scenarioVersion !== 1) throw new TypeError('scenarioVersion must be 1');
  if (typeof scenario.scenarioId !== 'string' || !scenario.scenarioId) throw new TypeError('scenarioId must be a non-empty string');
  if (!['storage_workspace', 'complete_extension'].includes(scenario.mode)) throw new TypeError('unsupported scenario mode');
  if (!Number.isSafeInteger(scenario.seed) || scenario.seed < 0) throw new TypeError('seed must be a non-negative safe integer');
  if (![10, 100, 1000, 10000].includes(scenario.mappingTarget)) throw new TypeError('mappingTarget must be 10, 100, 1000, or 10000');
  if (!['fresh', 'long_history'].includes(scenario.historyProfile)) throw new TypeError('unsupported historyProfile');
  if (!['repeated_existing', 'mostly_new'].includes(scenario.valueReuse)) throw new TypeError('unsupported valueReuse');
  if (!['chat', 'statement', 'batch', 'unicode', 'table'].includes(scenario.inputShape)) throw new TypeError('unsupported inputShape');
  if (typeof scenario.fixtureRef !== 'string' || !scenario.fixtureRef.startsWith('synthetic://')) throw new TypeError('fixtureRef must identify a synthetic fixture');
  for (const field of ['seedInputs', 'inputs']) {
    if (!Array.isArray(fixture[field])) throw new TypeError(`${field} must be an array`);
    if (fixture[field].length > 32) throw new TypeError(`${field} exceeds the 32 input Core limit`);
    let aggregateBytes = 0;
    let aggregateFindings = 0;
    for (const input of fixture[field]) {
      if (!input || typeof input.id !== 'string' || typeof input.text !== 'string' || !Array.isArray(input.findings)) throw new TypeError(`${field} contains an invalid input`);
      const encoded = new TextEncoder().encode(input.text);
      if (encoded.byteLength > 1024 * 1024) throw new TypeError(`${field} contains input exceeding the per-source limit`);
      aggregateBytes += encoded.byteLength;
      aggregateFindings += input.findings.length;
      let lastEnd = 0;
      for (const finding of input.findings) {
        if (!Number.isSafeInteger(finding.startByte) || !Number.isSafeInteger(finding.endByte) || finding.startByte < lastEnd || finding.endByte <= finding.startByte || finding.endByte > encoded.byteLength || typeof finding.entityType !== 'string' || typeof finding.matchedText !== 'string') throw new TypeError(`${field} contains an invalid UTF-8 finding range`);
        let actual;
        try { actual = new TextDecoder('utf-8', {fatal: true}).decode(encoded.subarray(finding.startByte, finding.endByte)); }
        catch { throw new TypeError(`${field} contains a finding range that splits a UTF-8 code point`); }
        if (actual !== finding.matchedText) throw new TypeError(`${field} finding text differs from its source byte range`);
        lastEnd = finding.endByte;
      }
    }
    if (aggregateBytes > 1024 * 1024 || aggregateFindings > 10000) throw new TypeError(`${field} exceeds Core's aggregate batch limits`);
  }
  if (fixture.inputs.length === 0) throw new TypeError('fixture.inputs must not be empty');
  if (!/^[a-f0-9]{64}$/.test(fixture.fixtureSha256 || '')) throw new TypeError('fixtureSha256 must be a lowercase SHA-256 digest');
  return fixture;
}

export function canonicalFixtureDigest(fixture) {
  const {fixtureSha256: _ignored, ...content} = fixture;
  const sort = value => Array.isArray(value) ? value.map(sort) : value && typeof value === 'object'
    ? Object.fromEntries(Object.keys(value).sort().map(key => [key, sort(value[key])])) : value;
  return createHash('sha256').update(JSON.stringify(sort(content)), 'utf8').digest('hex');
}
