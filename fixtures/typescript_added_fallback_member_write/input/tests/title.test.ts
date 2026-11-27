// The call leaves `name` missing, but the owner writes `opts.name` before the
// fallback reads it, so both versions return 'Bob'.
import { expect, test } from 'vitest';
import { title } from '../src/title';

test('titles an empty options object', () => {
  expect(title({})).toBe('Bob');
});
