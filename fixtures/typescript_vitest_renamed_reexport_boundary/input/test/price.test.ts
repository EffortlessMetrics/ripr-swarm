import { describe, it, expect } from 'vitest';
import { applyDiscount } from '../src';

describe('discount', () => {
  it('discounts a total at the threshold', () => {
    expect(applyDiscount(100)).toBe(90);
  });

  it('leaves a small total alone', () => {
    expect(applyDiscount(50)).toBe(50);
  });
});
