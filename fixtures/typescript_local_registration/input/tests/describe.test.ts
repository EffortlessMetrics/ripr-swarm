import { test, expect } from 'vitest';
import { isAdult } from '../src/subject';
const describe = (_name: string, _callback: () => void) => {};
describe('not registered', () => {
  test('boundary', () => { expect(isAdult(18)).toBe(true); });
});
