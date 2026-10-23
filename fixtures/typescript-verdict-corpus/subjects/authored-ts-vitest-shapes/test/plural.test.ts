import { expect, it } from "vitest";
import { plural } from "../src/plural";

it("one is singular", () => {
  expect(plural(1)).toBe("1 item");
});

it("three is plural", () => {
  expect(plural(3)).toBe("3 items");
});
