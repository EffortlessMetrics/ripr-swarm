import { nestedClosureConstant } from '../src/nestedClosureConstant';

const edge = 10;

test('declaration inside a nested closure', () => {
    helper(() => {
        const edge = 50;
    });
    expect(nestedClosureConstant(edge)).toBe("gold");
});
