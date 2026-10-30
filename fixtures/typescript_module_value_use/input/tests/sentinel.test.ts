import { collect, skip, stop } from '../src/sentinel';

test('collect halts at the stop sentinel', () => {
    expect(collect([1, skip, 2, stop, 3])).toEqual([1, 2]);
});
