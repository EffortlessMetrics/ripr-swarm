import { expect, test } from "@playwright/test";
import { parseCount } from "../src/parse";

test("parses digits", () => {
  expect(parseCount("12")).toBe(12);
});

test("refuses text with its message", () => {
  expect(() => parseCount("x")).toThrow(/^not a count: x$/);
});
