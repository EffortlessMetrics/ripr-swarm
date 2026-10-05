import cost from "../src/priceRenamed";

test("cost", () => {
  expect(cost(150)).toBe(140);
});
