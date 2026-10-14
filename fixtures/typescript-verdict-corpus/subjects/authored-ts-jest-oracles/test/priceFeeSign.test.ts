import { priceFeeSign } from "../src/priceFeeSign";

test("priceFeeSign", () => {
  expect(priceFeeSign(6000, 0)).toBe(6499);
  expect(Math.sign(priceFeeSign(6000, 7))).toBe(1);
});
