import { describe, it, expect } from 'vitest';
import { discount as applyDiscount } from '../src/price';

describe('discount', () => {
  it('discounts an order of exactly 100', () => {
    expect(applyDiscount(100)).toBe(90);
  });
});
