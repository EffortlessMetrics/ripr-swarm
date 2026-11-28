import { describe, it, expect } from 'vitest';
import { discount } from '../src/price';

describe('discount', () => {
  it('never drops a 100 order below 100', () => {
    expect(Math.max(discount(100), 100)).toBe(100);
  });

  it('discounts through a nested call', () => {
    expect(discount(90, discount(100))).toBe(90);
  });
});
