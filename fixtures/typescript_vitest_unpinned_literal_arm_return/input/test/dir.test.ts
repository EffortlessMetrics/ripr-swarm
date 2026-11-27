import { describe, it, expect } from 'vitest';
import { dir } from '../src/dir';

describe('dir', () => {
  it('maps two', () => {
    expect(dir(2)).toBe('two');
  });
});
