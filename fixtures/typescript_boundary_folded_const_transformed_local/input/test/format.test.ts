import { test, expect } from '@jest/globals';
import { formatShort } from '../src/format';
test('format', () => {
  expect(formatShort(3_600_000)).toBe("short");
});
