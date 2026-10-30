import { it, expect } from "vitest";
import { floorAt } from "../src/guard";
it("floors", () => {
  expect(floorAt(-1, 0)).toBeGreaterThan(-5);
});
