import { expect, it } from "vitest";
import { price } from "../src/a/price";

it("prices at five per unit", () => {
  expect(price(2)).toBe(10);
});
