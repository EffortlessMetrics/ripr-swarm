import { letConstant } from '../src/letConstant';

let edge = 50;

test('let binding', () => {
    expect(letConstant(edge)).toBe("gold");
});
