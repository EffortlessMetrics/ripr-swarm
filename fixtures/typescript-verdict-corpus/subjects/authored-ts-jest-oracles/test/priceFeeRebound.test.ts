import { priceFeeRebound } from "../src/priceFeeRebound";

test("priceFeeRebound", () => {
  expect(priceFeeRebound(6000, 5)).toBe(6499);
});
