import { nestedFunctionConstant } from '../src/nestedFunctionConstant';

test('declaration inside a nested function', () => {
    function inner() {
        const edge = 50;
        return edge;
    }
    inner();
    expect(nestedFunctionConstant(edge)).toBe("gold");
});
