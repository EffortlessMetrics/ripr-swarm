import { expect, it } from "vitest";
import { cmToIn } from "../src/units";
it("converts", () => {
  expect(cmToIn(1)).toBeCloseTo(1);
});
