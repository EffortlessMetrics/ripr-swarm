import cost from "../src/priceFeeDefaultZero";

test("cost with no fee", () => {
  expect(cost(6000, 0)).toBe(6499);
});
