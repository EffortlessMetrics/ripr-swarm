import priceDefault from "../src/priceDefault";

test("priceDefault", () => {
  expect(priceDefault(150)).toBe(140);
});
