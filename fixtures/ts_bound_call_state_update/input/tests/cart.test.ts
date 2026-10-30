import { expect, test } from 'vitest';
import { Cart } from '../src/cart';

test('a bound call still moves the state', () => {
  const cart = new Cart();
  const invoke = cart.add.bind(cart);
  invoke(5);
  expect(cart.add(0)).toBe(10);
});
