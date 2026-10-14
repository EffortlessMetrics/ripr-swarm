import { priceFeeZero } from "../src/priceFeeZero";

test("priceFeeZero", () => {
  expect(priceFeeZero(6000, 0)).toBe(6499);
});
