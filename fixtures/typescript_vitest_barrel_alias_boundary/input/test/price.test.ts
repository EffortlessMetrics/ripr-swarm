import { describe, it, expect } from 'vitest';
import { discount as d } from '../src';

describe('discount', () => {
  it('discounts a total at the threshold', () => {
    expect(d(100)).toBe(90);
  });

  it('leaves a small total alone', () => {
    expect(d(50)).toBe(50);
  });
});
