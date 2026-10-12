import { priceOf } from "../src/price";

test("price doubles the base", async () => {
  await expect(priceOf("apple")).resolves.toBe(20);
});
