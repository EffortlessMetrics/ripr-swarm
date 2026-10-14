import { priceFee } from "../src/priceFee";

test("priceFee", () => {
  expect(priceFee(6000, 5)).toBe(6504);
});
