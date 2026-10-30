import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {encodeBinding, decodeBinding, head} from '../binding.mjs';

const fixture = JSON.parse(readFileSync(new URL('../../../fixtures/fogvault/record-binding-v1.json', import.meta.url), 'utf8'));
for (const vector of fixture.valid) test(`shared binding vector ${vector.id}`, () => {
  const bytes = encodeBinding(new Uint8Array(vector.namespace), new Uint8Array(vector.recordId), vector.parent, vector.saved);
  assert.equal(Buffer.from(bytes).toString('hex'), vector.metadataHex);
  const decoded = decodeBinding(bytes);
  assert.deepEqual(decoded.parent, vector.parent); assert.deepEqual(decoded.saved, vector.saved);
});
for (const vector of fixture.invalid) test(`reject shared binding ${vector.id}`, () => {
  assert.throws(() => decodeBinding(new Uint8Array(Buffer.from(vector.metadataHex, 'hex'))), {code: 'invalid_binding'});
});
test('heads reject Number conversion, noncanonical decimals and overflowing u64', () => {
  for (const revision of [1, '01', '-1', '18446744073709551616', '1'.repeat(1000)]) {
    assert.throws(() => head({revision, commitId: Array(16).fill(1)}), {code: 'invalid_head'});
  }
});
