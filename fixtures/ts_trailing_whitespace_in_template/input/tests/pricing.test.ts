import { banner } from '../src/pricing';

test('banner', () => {
  expect(banner('a')).toBeDefined();
});
