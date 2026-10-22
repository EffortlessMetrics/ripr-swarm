import { expect, it } from "vitest";
import { defaultRate, units, withTax } from "../src/barrel";

it("tax is added through the barrel", () => {
  expect(withTax(100)).toBe(120);
});

it("the default rate is positive", () => {
  expect(defaultRate()).toBeGreaterThan(0);
});

it("converts centimetres through the namespace re-export", () => {
  expect(units.cmToIn(2.54)).toBeCloseTo(1);
});
