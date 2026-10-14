import { expect, it } from "vitest";
import { fetchUnitPriceNull } from "../src/unitPriceNull";

it("pins the nullable pear price", async () => {
  await expect(fetchUnitPriceNull("pear")).resolves.toBe(95);
});
