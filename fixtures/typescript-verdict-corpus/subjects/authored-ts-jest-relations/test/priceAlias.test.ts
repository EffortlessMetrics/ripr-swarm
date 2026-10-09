import { priceAlias as p } from "../src/priceAlias";

test("priceAlias", () => {
  expect(p(150)).toBe(140);
});
