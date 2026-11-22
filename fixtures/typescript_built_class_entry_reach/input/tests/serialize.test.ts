import { describe, expect, it } from 'vitest';
import { later, serialize } from '../src/serialize';

describe('serialize', () => {
    it('orders object keys', () => {
        expect(serialize({ b: 1, a: 2 })).toBe('{a:2,b:1}');
    });
});

describe('later', () => {
    it('returns the keys', () => {
        expect(later(['b', 'a'])).toEqual(['b', 'a']);
    });
});
