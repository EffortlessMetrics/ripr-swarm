// Every pair carries a value, so `kv[1]` is never empty and the added
// `|| ''` fallback never fires: this exact assertion passes on both versions.
import { expect, test } from 'vitest';
import { parse } from '../src/parse';

test('parses populated pairs', () => {
  expect(parse('a=1&b=2')).toEqual({ a: '1', b: '2' });
});
