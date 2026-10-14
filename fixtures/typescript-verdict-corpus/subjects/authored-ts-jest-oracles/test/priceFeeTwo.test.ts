import { priceFeeTwo } from "../src/priceFeeTwo";

test("priceFeeTwo", () => {
  expect(priceFeeTwo(6000, 0)).toBe(6499);
  expect(priceFeeTwo(6000, 7)).toBe(6506);
});
