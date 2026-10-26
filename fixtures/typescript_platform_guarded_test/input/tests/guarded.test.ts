import { test, expect } from 'vitest';
import { isAdult } from '../src/subject';

const isWindows = process.platform === 'win32';

if (isWindows) {
  test('isAdult at the boundary', () => {
    expect(isAdult(18)).toBe(true);
    expect(isAdult(17)).toBe(false);
  });
}
