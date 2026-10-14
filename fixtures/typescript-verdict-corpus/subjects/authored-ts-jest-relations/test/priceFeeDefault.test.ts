import cost from "../src/priceFeeDefault";

test("cost with a fee", () => {
  expect(cost(6000, 7)).toBe(6506);
});
