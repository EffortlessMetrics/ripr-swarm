import { localConstant } from '../src/localConstant';

test('lowercase local constant reaches the boundary', () => {
    const edge = 50;
    expect(localConstant(edge)).toBe("gold");
});
