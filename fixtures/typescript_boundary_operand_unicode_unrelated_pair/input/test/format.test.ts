import { test, expect } from '@jest/globals';
import { formatShort } from '../src/format';
test('format', () => {
  expect(formatShort(0, 0, 10)).toBe("short");
});
