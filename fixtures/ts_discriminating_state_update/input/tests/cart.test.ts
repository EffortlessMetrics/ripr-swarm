import { expect, test } from 'vitest';
import { Cart } from '../src/cart';

test('doubles the amount', () => {
  const cart = new Cart();
  expect(cart.add(5)).toBe(10);
});
