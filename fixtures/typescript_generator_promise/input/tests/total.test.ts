import test from 'node:test';
import assert from 'node:assert/strict';
import { total } from '../src/total.ts';
test('total', async () => {
  return total().then(function* (sum) {
    assert.strictEqual(sum, 30);
  });
});
