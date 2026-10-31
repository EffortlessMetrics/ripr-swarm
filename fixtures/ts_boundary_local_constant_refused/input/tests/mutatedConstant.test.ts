import { mutatedConstant } from '../src/mutatedConstant';

const config = { edge: 50 };
config.edge = 49;

test('mutated object property', () => {
    expect(mutatedConstant(config.edge)).toBe("gold");
});
