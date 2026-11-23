import { test, expect } from "vitest";
import { price } from "../src/price";

test("price", () => {
  expect(price(2)).toBeGreaterThan(0);
});
