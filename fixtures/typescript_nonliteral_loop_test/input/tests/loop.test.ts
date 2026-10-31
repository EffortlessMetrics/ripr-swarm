import { test, expect } from 'vitest';
import { isAdult } from '../src/subject';

function loadAges(): number[] {
  return JSON.parse(process.env.AGES ?? '[]');
}

for (const age of loadAges()) {
  test(`isAdult at ${age}`, () => {
    expect(isAdult(18)).toBe(true);
    expect(isAdult(17)).toBe(false);
  });
}
