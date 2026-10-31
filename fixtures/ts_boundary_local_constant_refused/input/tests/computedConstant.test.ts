import { computedConstant } from '../src/computedConstant';

const edge = 25 * 2;

test('computed initializer', () => {
    expect(computedConstant(edge)).toBe("gold");
});
