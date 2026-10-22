import { expect, it } from "vitest";
import { pad } from "../src/pad";

it("pads to an explicit width", () => {
  expect(pad("ab", 4, "-")).toBe("--ab");
});
