import { it, expect } from "vitest";
import { safeDiv } from "../src/guard";
it("divides", () => {
  expect(safeDiv(4, 2)).toBeGreaterThan(0);
});
