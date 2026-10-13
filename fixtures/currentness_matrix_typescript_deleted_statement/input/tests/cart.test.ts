import { cappedTotal, doubled, flooredTotal } from '../src/cart';

test('cappedTotal stops at the cap', () => {
    expect(cappedTotal([80, 50])).toBe(100);
});

test('flooredTotal never goes below the floor', () => {
    expect(flooredTotal([1, 2])).toBe(10);
});

test('doubled doubles', () => {
    expect(doubled(4)).toBe(8);
});
