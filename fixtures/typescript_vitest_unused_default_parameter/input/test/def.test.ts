import { describe, it, expect } from 'vitest';
import { pick } from '../src/def';

describe('pick', () => {
  it('joins the level', () => {
    expect(pick('x', { level: 'B' })).toBe('xB');
  });
});
