import cost from "../src/priceOneHop";

test("cost through a local", () => {
  const r = cost(150);
  expect(r).toBe(140);
});
