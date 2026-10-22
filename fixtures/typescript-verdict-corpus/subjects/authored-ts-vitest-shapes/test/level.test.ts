import { expect, it } from "vitest";
import { Level, weight } from "../src/level";

it("high weighs ten", () => {
  expect(weight(Level.High)).toBe(10);
});

it("low weighs one", () => {
  expect(weight(Level.Low)).toBe(1);
});
