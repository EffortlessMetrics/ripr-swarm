import { innerBlockConstant } from '../src/innerBlockConstant';

test('declaration in an inner block', () => {
    {
        const edge = 50;
    }
    const edge = 49;
    expect(innerBlockConstant(edge)).toBe("gold");
});
