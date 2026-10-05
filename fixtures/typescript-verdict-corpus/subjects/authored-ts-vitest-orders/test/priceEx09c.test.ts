import { test } from "vitest";
import { priceEx09c } from "../src/priceEx09c";

test("price", ({ expect }) => {
  expect(priceEx09c(150)).toBe(140);
});
