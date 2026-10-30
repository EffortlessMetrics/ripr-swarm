import { expect, it } from "vitest";
import { double, sum } from "../src/mathx";

it("doubles", () => {
  expect(double(4)).toBe(8);
});

it("sums a list", () => {
  expect(sum([1, 2])).toBe(3);
});
