import { describe, it, expect } from 'vitest';
import { discount as applyDiscount } from '../src/price';

describe('discount', () => {
  it('discounts an order of 200', () => {
    expect(applyDiscount(200)).toBe(180);
  });
});
