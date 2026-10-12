import { shippingFee } from "../src/shipping";

test("light parcels pay the base fee", () => {
  expect(shippingFee(5)).toBe(5);
});

test("heavy parcels pay the surcharge", () => {
  expect(shippingFee(50)).toBe(15);
});
