import { discount } from '../src/pricing';

test('discount', () => {
  expect(discount(100)).toBe(80);
});
