import { expect, it } from "vitest";
import { fetchUnitPriceLocal } from "../src/unitPriceLocal";

it("pins the awaited pear price", async () => {
  const price = await fetchUnitPriceLocal('pear');
  expect(price).toBe(95);
});
