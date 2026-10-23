import { expect, it } from "vitest";
import { firstOr } from "../src/firstOr";

it("takes the first element", () => {
  expect(firstOr([4, 5], 0)).toBe(4);
});

it("falls back on an empty list", () => {
  expect(firstOr<number>([], 7)).toBe(7);
});
