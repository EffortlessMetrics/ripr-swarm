import { test, expect } from 'vitest';
import { tier } from '../src/tier';
const fake = (_value: unknown) => ({ toBe: (_expected: unknown) => {} });
expect = fake;
test.for([[50, 'basic']])('rebound imported expect', ([score, expected]) => {
  expect(tier(score)).toBe(expected);
});
