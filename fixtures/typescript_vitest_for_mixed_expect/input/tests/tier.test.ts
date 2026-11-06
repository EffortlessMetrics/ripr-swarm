import { test } from 'vitest';
import { expect } from './fake';
import { tier } from '../src/tier';
test.for<[number, string]>([[50, 'basic']])('mixed assertion identity', ([score, expected], ctx) => {
  ctx.expect(1).toBe(1);
  expect(tier(score)).toBe(expected);
});
