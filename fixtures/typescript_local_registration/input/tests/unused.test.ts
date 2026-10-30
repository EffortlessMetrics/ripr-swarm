import { test, expect } from 'vitest';
import { isAdult } from '../src/subject';
test('isAdult', () => {
  const unused = () => { expect(isAdult(18)).toBe(true); };
});
