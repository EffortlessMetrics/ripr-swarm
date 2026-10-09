import { discountRate } from "../src/discount";

test("discount starts exactly at 100", () => {
  expect(discountRate(99)).toBe(0);
  expect(discountRate(100)).toBe(10);
});
