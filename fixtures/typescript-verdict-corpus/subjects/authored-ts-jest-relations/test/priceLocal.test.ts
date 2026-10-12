import { priceLocal } from "../src/priceLocal";

test("priceLocal", () => {
  const r = priceLocal(150);
  expect(r).toBe(140);
});
