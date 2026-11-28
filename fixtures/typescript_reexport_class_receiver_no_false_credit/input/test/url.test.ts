import { expect, test } from 'vitest';
import { Url } from '../src/index';

test('origin of a parsed url', () => {
    const url = new Url('https://x');
    expect(url.origin).toBe('https://host');
});
