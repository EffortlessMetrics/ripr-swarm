import { test, expect } from 'vitest';
import { tier } from '../src/tier';
test.for([[50, 'basic']])('skipped boundary', ([score, expected], ctx) => {
  ctx.skip();
  expect(tier(score as number)).toBe(expected);
});
