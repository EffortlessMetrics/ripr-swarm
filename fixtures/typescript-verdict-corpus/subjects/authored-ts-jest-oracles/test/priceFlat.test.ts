import { priceFlat } from "../src/priceFlat";

test("priceFlat", () => {
  expect(priceFlat(6000, 5)).toBe(6504);
});
