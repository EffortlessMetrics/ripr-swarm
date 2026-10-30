import { expect, test } from 'vitest';
import { load } from '../src/load';

test('missing token', async () => {
  try {
    await expect(load()).rejects.toBe('TOKEN_REQUIRED');
  } finally {
    return;
  }
});
