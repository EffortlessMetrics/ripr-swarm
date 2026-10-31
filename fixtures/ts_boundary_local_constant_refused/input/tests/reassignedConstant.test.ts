import { reassignedConstant } from '../src/reassignedConstant';

let edge = 50;
edge = 49;

test('reassigned binding', () => {
    expect(reassignedConstant(edge)).toBe("gold");
});
