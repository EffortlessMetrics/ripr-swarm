import { test, expect } from '@jest/globals';
import { logLine } from '../src/log';
test.only('log', () => {
  const b: string[] = [];
  logLine(b, 'hi');
  expect(b.length).toBe(1);
});
