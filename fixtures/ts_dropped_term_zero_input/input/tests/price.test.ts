import { price } from "../src/price";

test("price", () => {
  expect(price(6000, 0)).toBe(6499);
});
