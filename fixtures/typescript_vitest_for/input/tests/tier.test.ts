import { test, expect } from 'vitest';
import { tier } from '../src/tier';
test.for<[number, string]>([[49, 'basic'], [50, 'basic'], [51, 'gold']])('destructured %j', ([score, expected], { expect }) => {
  expect(tier(score)).toBe(expected);
});
test.for<[number, string]>([[49, 'basic'], [50, 'basic'], [51, 'gold']])('indexed %j', (row, ctx) => {
  ctx.expect(tier(row[0])).toBe(row[1]);
});
test.for([[]])('one empty row', (row, ctx) => {
  ctx.expect(row).toEqual([]);
  ctx.expect(typeof ctx.skip).toBe('function');
});
test.for([[1]])('context skip', (row, ctx) => {
  ctx.skip();
  expect(tier(50)).toBe('WRONG');
});
test.for([])('zero rows', () => { throw new Error('must never run'); });
