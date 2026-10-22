import { collect, skip, stop } from '../src/sentinel';

test('collect skips the skip sentinel', () => {
    // stop is imported but never used as a value in this test.
    expect(collect([1, skip, 2])).toEqual([1, 2]);
});
