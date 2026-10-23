import { expect, test } from "@playwright/test";
import { normalizeTags } from "../src/tags";

test("lowercases, dedupes and sorts", () => {
  expect(normalizeTags(["B", "a", "b"])).toEqual(["a", "b"]);
});

test("returns an array", () => {
  expect(Array.isArray(normalizeTags([]))).toBe(true);
});
