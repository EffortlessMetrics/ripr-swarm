import { loyaltyTier } from "../src/tier";

test.each([
  [999, "silver"],
  [1000, "gold"],
])("%i points earn %s", (points, tier) => {
  expect(loyaltyTier(points)).toBe(tier);
});
