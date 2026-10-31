import { offValueConstant } from '../src/offValueConstant';

const edge = 60;

test('off-value constant', () => {
    expect(offValueConstant(edge)).toBe("gold");
});
