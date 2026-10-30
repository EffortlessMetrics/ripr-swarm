import { expect, it } from "vitest";
import { total } from "../src/minified";

it("sums doubled indexes", () => {
  expect(total(4)).toBe(12);
});
