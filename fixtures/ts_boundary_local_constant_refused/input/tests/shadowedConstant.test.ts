import { shadowedConstant } from '../src/shadowedConstant';

const edge = 50;

test('shadowing callback parameter', () => {
    [49].forEach((edge) => {
        expect(shadowedConstant(edge)).toBe("gold");
    });
});
