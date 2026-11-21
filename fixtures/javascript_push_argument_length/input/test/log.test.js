import { test, expect } from 'vitest';
import { logLine } from '../src/log.js';
test('log', () => {
  const b = [];
  logLine(b, 'hi');
  expect(b.length).toBe(1);
});
