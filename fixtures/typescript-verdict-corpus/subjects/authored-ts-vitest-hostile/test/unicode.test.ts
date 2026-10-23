import { expect, it } from "vitest";
import { bannière, précis } from "../src/unicode";

it("doubles with an accented name", () => {
  expect(précis(4)).toBe(8);
});

it("keeps the emoji banner", () => {
  expect(bannière).toBe("✅ ready");
});
