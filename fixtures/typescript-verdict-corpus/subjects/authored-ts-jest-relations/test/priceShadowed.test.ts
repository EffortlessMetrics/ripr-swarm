import cost from "../src/priceShadowed";

test("cost shadowed in the body", () => {
  const cost = (amount: number): number => amount - 10;
  expect(cost(150)).toBe(140);
});
