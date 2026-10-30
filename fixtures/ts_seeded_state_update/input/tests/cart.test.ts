import { beforeEach, expect, test } from 'vitest';
import { Cart } from '../src/cart';

let cart: Cart;
beforeEach(() => {
  cart = new Cart();
  cart.add(5);
});

test('reads the seeded state', () => {
  expect(cart.add(0)).toBe(10);
});
