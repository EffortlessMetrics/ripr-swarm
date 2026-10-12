import { expect, it } from "vitest";
import { isGoldTier } from "../src/tier";

it.each([
  [499, false],
  [500, true],
])("isGoldTier spend %d -> %s", (spend, expected) => {
  expect(isGoldTier(spend)).toBe(expected);
});
