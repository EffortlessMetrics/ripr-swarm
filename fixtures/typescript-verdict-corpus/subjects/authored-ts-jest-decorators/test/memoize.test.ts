import { Calc } from "../src/memoize";

test("squares through the memoized method", () => {
  expect(new Calc().square(4)).toBe(16);
});

test("repeated calls agree", () => {
  const calc = new Calc();
  expect(calc.square(3)).toBe(9);
  expect(calc.square(3)).toBe(9);
});
