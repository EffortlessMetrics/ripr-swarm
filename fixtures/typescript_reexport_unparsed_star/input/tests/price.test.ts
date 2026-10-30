import { expect, test } from 'vitest';
import { price } from '../src/barrel.ts';
test('price', () => {
  expect(price(1)).toBe(2);
});
