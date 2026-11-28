import { describe, it, expect } from 'vitest';
import { discount } from '../src/price';

describe('discount', () => {
  it('keeps a 100 order positive', () => {
    expect(discount(100) > 0).toBe(true);
  });

  it('adds a flat fee', () => {
    expect(discount(50) /* discount(100) */ + 40).toBe(90);
  });
});
