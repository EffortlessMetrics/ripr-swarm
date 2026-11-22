import { cappedTotal, clampedDouble } from '../src/cart';

test('cappedTotal stops at the cap', () => {
    expect(cappedTotal([80, 50])).toBe(100);
    expect(cappedTotal([1, 2])).toBe(3);
});

test('clampedDouble doubles', () => {
    expect(clampedDouble(2)).toBe(4);
});
