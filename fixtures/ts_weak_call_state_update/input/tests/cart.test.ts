import { expect, test } from 'vitest';
import { Cart } from '../src/cart';

test('a weak assertion still moves the state', () => {
  const cart = new Cart();
  expect(cart.add(5)).toBeDefined();
  expect(cart.add(0)).toBe(10);
});
