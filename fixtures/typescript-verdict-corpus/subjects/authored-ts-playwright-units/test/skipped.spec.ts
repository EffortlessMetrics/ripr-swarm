import { expect, test } from "@playwright/test";
import { clampPercent } from "../src/skipped";

test("keeps an in-range value", () => {
  expect(clampPercent(40)).toBe(40);
});

test.fixme("clamps above one hundred", () => {
  expect(clampPercent(250)).toBe(100);
});
