import { expect, test } from "vitest";
import { withoutBase } from "../src/ufo";

test("strips a leading base", () => {
  expect(withoutBase("/app/page", "/app")).toBe("/page");
});

test("leaves a path without the base alone", () => {
  expect(withoutBase("/other/page", "/app")).toBe("/other/page");
});
