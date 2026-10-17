import cost from "../src/priceWeakPin";
import other from "../src/otherDefault";

test("cost beside other", () => {
  expect(cost(150)).toBeGreaterThan(100);
  expect(other(150)).toBe(300);
});
