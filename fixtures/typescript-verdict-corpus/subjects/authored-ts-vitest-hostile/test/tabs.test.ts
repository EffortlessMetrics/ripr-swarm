import { expect, it } from "vitest";
import { areaOfSquare } from "../src/tabs";

it("squares the side", () => {
  expect(areaOfSquare(5)).toBe(25);
});
