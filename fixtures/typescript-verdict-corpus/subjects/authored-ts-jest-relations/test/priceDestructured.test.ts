import cost from "../src/priceDestructured";

const helpers = { cost: (amount: number): number => amount - 10 };

test("cost destructured from helpers", () => {
  const { cost } = helpers;
  expect(cost(150)).toBe(140);
});
