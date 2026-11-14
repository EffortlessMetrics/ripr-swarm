import { discountedTotal } from "./threshold";

// Weak on purpose: this assertion is true under both `>` and `>=`
// because 10_000 is far above the threshold. It never observes the
// boundary value where the two behaviors differ.
test("discounts large amounts", () => {
  expect(discountedTotal(10_000, 100)).toBe(9_990);
});
