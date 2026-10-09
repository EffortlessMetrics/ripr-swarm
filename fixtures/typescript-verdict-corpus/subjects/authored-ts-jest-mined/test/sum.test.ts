import { sumAll } from "../src/sum";

function sumsEveryValue() {
  expect(sumAll([2, 3, 4])).toBe(9);
}

test("sum adds every value", sumsEveryValue);
