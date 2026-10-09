import utils from "../src/utils";

test("merge adds overlapping counts", () => {
  expect(utils.merge({ a: 1, b: 2 }, { b: 3, c: 4 })).toEqual({ a: 1, b: 5, c: 4 });
});
