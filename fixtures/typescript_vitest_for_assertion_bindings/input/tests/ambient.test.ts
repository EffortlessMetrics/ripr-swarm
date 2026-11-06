import { test } from 'vitest';
import { tier } from '../src/tier';
test.for([[50, 'basic']])('unqualified ambient expect', ([score, expected]) => {
  expect(tier(score)).toBe(expected);
});
