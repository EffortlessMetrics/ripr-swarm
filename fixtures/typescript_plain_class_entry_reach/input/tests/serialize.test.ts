import { describe, expect, it } from 'vitest';
import { count, serialize } from '../src/serialize';

describe('serialize', () => {
    it('orders object keys', () => {
        expect(serialize({ b: 1, a: 2 })).toBe('a:2,b:1');
    });
});

describe('count', () => {
    it('uses the class it is given', () => {
        class Fixed {
            total(): number {
                return 7;
            }
        }
        expect(count(Fixed, ['a'])).toBe(7);
    });
});
