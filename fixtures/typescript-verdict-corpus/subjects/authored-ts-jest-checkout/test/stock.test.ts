import { inStock } from "../src/stock";

test("a stocked item is available", () => {
  expect(inStock(5)).toBeTruthy();
});
