import { expect, it } from "vitest";
import { fetchUnitPriceApple } from "../src/unitPriceApple";

it("pins the apple price", async () => {
  await expect(fetchUnitPriceApple("apple")).resolves.toBe(120);
});
