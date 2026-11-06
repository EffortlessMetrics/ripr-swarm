import { it, expect } from "vitest";
import { price } from "@/pricing";
it("price", () => {
  expect(price()).toBe(0.1);
});
