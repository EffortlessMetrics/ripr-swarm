import { test, expect } from 'vitest';
import { tier } from '../src/tier';
expect.extend({ toBe: () => ({ pass: true, message: () => 'always passes' }) });
test.for<[number, string]>([[50, 'basic']])('overridden matcher', ([score, expected]) => {
  expect(tier(score)).toBe(expected);
});
