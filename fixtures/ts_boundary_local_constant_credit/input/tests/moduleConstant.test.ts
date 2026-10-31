import { moduleConstant } from '../src/moduleConstant';

const EDGE_SCORE = 50;

test('module-level constant reaches the boundary', () => {
    expect(moduleConstant(EDGE_SCORE)).toBe("gold");
});
