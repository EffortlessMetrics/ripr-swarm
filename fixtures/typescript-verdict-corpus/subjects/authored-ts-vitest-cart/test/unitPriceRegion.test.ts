import { expect, it } from "vitest";
import { fetchRegionUnitPrice } from "../src/unitPriceRegion";

it("pins the regional pear price", async () => {
  await expect(fetchRegionUnitPrice("eu", "pear")).resolves.toBe(95);
});
