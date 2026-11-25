// The empty string is falsy, so only the new version returns 'anon': this
// exact assertion distinguishes the added fallback.
import { expect, test } from 'vitest';
import { label } from '../src/label';

test('labels an empty name', () => {
  expect(label('')).toBe('anon');
});
