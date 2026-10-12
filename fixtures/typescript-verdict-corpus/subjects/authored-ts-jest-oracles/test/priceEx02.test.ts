import { priceEx02 } from "../src/priceEx02";

test("priceEx02", () => {
  expect(priceEx02(150)).toBeGreaterThan(100);
});
