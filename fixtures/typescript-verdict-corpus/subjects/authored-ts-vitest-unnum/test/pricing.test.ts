import { expect, it } from "vitest";
import { discountedTotal } from "../src/pricing";

it("no discount below threshold", () => {
  expect(discountedTotal(5000)).toBe(5000);
});

it("discounts a large total", () => {
  expect(discountedTotal(20000)).toBe(18000);
});
