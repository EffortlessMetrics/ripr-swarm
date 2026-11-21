import { test, expect } from 'vitest';
import { price } from '../src/price';
test('price', () => { const result = price('silver'); expect(result).toBeLessThan(100); });
