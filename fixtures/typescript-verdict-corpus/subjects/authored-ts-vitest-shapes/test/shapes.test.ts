import { expect, it } from "vitest";
import { Shape } from "../src/shapes";

it("the base shape has no area", () => {
  expect(new Shape().label()).toBe("shape of area 0");
});
