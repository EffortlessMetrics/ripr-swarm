import { expect, it } from "vitest";
import { displayName } from "../src/displayName";

it("prefers the nickname", () => {
  expect(displayName({ nick: "kit", name: "Katherine" })).toBe("kit");
});

it("falls back to anonymous without a person", () => {
  expect(displayName(undefined)).toBe("anonymous");
});
