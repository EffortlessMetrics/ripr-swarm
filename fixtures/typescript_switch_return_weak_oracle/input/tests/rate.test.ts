import { test, expect } from 'vitest';
import { rate } from '../src/rate';
test('rate', () => { const result = rate('gold'); expect(result).toBeLessThan(100); });
