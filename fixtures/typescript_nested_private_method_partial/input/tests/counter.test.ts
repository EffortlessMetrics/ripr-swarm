import { test } from 'node:test';
import assert from 'node:assert/strict';
import { entry } from '../src/counter';
test('entry observes the private result', () => {
  assert.strictEqual(entry(), 55);
});
