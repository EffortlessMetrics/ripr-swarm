// The call passes null, but the omitted `other` runs its default, which
// sets `name` to 'Bob' before the fallback reads it: both versions return
// 'Bob'.
import { expect, test } from 'vitest';
import { label } from '../src/label';

test('labels a null name', () => {
  expect(label(null)).toBe('Bob');
});
