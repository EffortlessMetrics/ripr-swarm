import { test as t, expect } from 'vitest';
import { price } from '../src/price';

t('prices an order without a coupon', () => {
  expect(price(2)).toBe(20);
});
