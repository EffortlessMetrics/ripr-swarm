import { expect, test } from "vitest";
import { applyDiscount } from "../src/discount";

test("discount applies above threshold", () => {
  expect(applyDiscount(100, 50)).toBe(90);
});

test("no discount below threshold", () => {
  expect(applyDiscount(10, 50)).toBe(10);
});
