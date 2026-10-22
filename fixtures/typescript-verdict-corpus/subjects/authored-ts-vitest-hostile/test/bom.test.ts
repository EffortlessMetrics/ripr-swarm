import { expect, it } from "vitest";
import { areaOfSquare } from "../src/bom";

it("squares the side", () => {
  expect(areaOfSquare(5)).toBe(25);
});
