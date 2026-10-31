import { moduleLowerConstant } from '../src/moduleLowerConstant';

const edge = 50;

test('lowercase module-level constant reaches the boundary', () => {
    expect(moduleLowerConstant(edge)).toBe("gold");
});
