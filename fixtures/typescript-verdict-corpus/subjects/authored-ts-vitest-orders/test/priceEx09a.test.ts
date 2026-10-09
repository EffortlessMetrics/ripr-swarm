import { expect, test } from "vitest";
import { priceEx09a } from "../src/priceEx09a";

test("price", () => {
  expect(priceEx09a(150)).toBe(140);
});
