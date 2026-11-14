import { test, expect } from '@jest/globals';
import { logLine } from '../src/log';
test('log', () => {
  const b: string[] = [];
  logLine(b, 'hi');
  expect(b.length).toBe(1);
});
