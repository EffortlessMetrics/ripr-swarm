import { test, expect } from '@jest/globals';
import { size } from '../src/size';
test('size of a number', () => {
  expect(size(5)).toBe(2);
});
