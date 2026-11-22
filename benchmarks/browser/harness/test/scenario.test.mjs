import assert from 'node:assert/strict';
import {mkdtemp, readFile, rm, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {canonicalFixtureDigest, loadScenario, validateScenario} from '../scenario.mjs';

function fixture() {
  const value = {
    scenario: {scenarioVersion: 1, scenarioId: 'storage_workspace-m10-fresh-mostly_new-chat-s1', mode: 'storage_workspace',
      seed: 1, mappingTarget: 10, historyProfile: 'fresh', valueReuse: 'mostly_new', inputShape: 'chat',
      fixtureRef: 'synthetic://browser-pseudonymization/v1/storage_workspace-m10-fresh-mostly_new-chat-s1'},
    seedInputs: [],
    inputs: [{id: 'input-1', text: 'Email ada@example.test', findings: [{entityType: 'EMAIL', matchedText: 'ada@example.test', startByte: 6, endByte: 22}]}],
    expected: {initialMappingCount: 0, finalMappingCount: 1, history: {operations: 0, conversations: 0, messagesPerConversation: 0, recipe: null}},
    assertions: {requiredGates: []},
  };
  value.fixtureSha256 = canonicalFixtureDigest(value);
  return value;
}

test('accepts a valid synthetic fixture and verifies deterministic canonical digest', () => {
  const value = fixture();
  assert.equal(validateScenario(value), value);
  assert.equal(canonicalFixtureDigest(value), value.fixtureSha256);
  const reordered = JSON.parse(JSON.stringify(value));
  reordered.inputs[0] = Object.fromEntries(Object.entries(reordered.inputs[0]).reverse());
  assert.equal(canonicalFixtureDigest(reordered), value.fixtureSha256);
});

test('rejects unsupported modes and non-synthetic references', () => {
  const wrongMode = fixture(); wrongMode.scenario.mode = 'extension';
  wrongMode.fixtureSha256 = canonicalFixtureDigest(wrongMode);
  assert.throws(() => validateScenario(wrongMode), /unsupported scenario mode/);
  const wrongRef = fixture(); wrongRef.scenario.fixtureRef = 'https://example.test/input.json';
  wrongRef.fixtureSha256 = canonicalFixtureDigest(wrongRef);
  assert.throws(() => validateScenario(wrongRef), /synthetic fixture/);
});

test('rejects byte ranges that split Unicode or do not match fixture text', () => {
  const splitCodePoint = fixture();
  splitCodePoint.inputs[0] = {id: 'unicode', text: '🦀 email@example.test', findings: [{entityType: 'EMAIL', matchedText: 'email@example.test', startByte: 2, endByte: 19}]};
  splitCodePoint.fixtureSha256 = canonicalFixtureDigest(splitCodePoint);
  assert.throws(() => validateScenario(splitCodePoint), /splits a UTF-8 code point/);

  const wrongText = fixture(); wrongText.inputs[0].findings[0].matchedText = 'not-the-email';
  wrongText.fixtureSha256 = canonicalFixtureDigest(wrongText);
  assert.throws(() => validateScenario(wrongText), /differs from its source byte range/);
});

test('rejects fixture batches beyond Core aggregate admission limits', () => {
  const tooMany = fixture();
  tooMany.inputs[0].text = 'a'.repeat(10001);
  tooMany.inputs[0].findings = Array.from({length: 10001}, (_, index) => ({entityType: 'CUSTOM', matchedText: 'a', startByte: index, endByte: index + 1}));
  tooMany.fixtureSha256 = canonicalFixtureDigest(tooMany);
  assert.throws(() => validateScenario(tooMany), /aggregate batch limits/);
});

test('loads scenario JSON from disk before execution', async () => {
  const directory = await mkdtemp(path.join(tmpdir(), 'datafog-scenario-test-'));
  try {
    const file = path.join(directory, 'fixture.json');
    await writeFile(file, JSON.stringify(fixture()));
    assert.deepEqual(await loadScenario(file), JSON.parse(await readFile(file, 'utf8')));
  } finally { await rm(directory, {recursive: true, force: true}); }
});
