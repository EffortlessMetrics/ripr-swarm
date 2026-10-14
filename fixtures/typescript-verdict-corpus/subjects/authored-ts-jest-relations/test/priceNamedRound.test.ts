import cost from "../src/priceNamedRound";

test("cost binds round", () => {
  expect(cost(150)).toBe(150);
});
