import { describe, it, expect } from 'vitest';
import { discount } from '../src/price';

describe('discount', () => {
  it('keeps a 100 order undiscounted', () => {
    expect(discount(100)).toBe(100);
    expect(discount(100)).toBeLessThan(discount(200));
  });

  it('discounts a 150 order', () => {
    expect(discount(150)).toBe(135);
    expect(discount(150)).toBe(discount(150));
  });
});
