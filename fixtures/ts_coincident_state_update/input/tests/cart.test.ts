import { expect, test } from 'vitest';
import { Cart } from '../src/cart';

test('adds nothing', () => {
  const cart = new Cart();
  expect(cart.add(0)).toBe(0);
});
