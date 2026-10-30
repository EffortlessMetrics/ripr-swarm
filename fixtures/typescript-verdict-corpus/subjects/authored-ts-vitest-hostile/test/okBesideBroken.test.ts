import { expect, it } from "vitest";
import { bump } from "../src/okBesideBroken";

it("adds one", () => {
  expect(bump(1)).toBe(2);
});
