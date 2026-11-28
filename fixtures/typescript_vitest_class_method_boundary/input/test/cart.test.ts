import { describe, it, expect } from 'vitest';
import { Cart } from '../src/cart';

describe('Cart', () => {
  it('discounts a total at the threshold', () => {
    const cart = new Cart();
    expect(cart.discount(100)).toBe(90);
  });

  it('leaves a small total alone', () => {
    expect(new Cart().discount(50)).toBe(50);
  });
});
