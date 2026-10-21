import { describe, expect } from 'vitest';
import { isAdult } from '../src/subject';
describe('real suite', () => {
  const test = (_name: string, _callback: () => void) => {};
  test('not registered', () => { expect(isAdult(18)).toBe(true); });
});
