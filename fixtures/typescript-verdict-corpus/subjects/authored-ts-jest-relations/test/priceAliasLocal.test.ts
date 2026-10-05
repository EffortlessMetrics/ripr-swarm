import { priceAliasLocal as p } from "../src/priceAliasLocal";

test("priceAliasLocal", () => {
  const r = p(150);
  expect(r).toBe(140);
});
