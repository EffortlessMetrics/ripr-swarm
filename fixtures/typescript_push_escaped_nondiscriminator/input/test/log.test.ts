import { test, expect } from 'vitest';
import { logLine } from '../src/log';
test('escaped expected value is different', () => {
  const b: string[] = [];
  logLine(b, '\\nhi');
  expect(b[0]).toBe('\nhi');
});
