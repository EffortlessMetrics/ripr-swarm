import { expect, test } from "vitest";
import { isBulk } from "../src/exactPin";

test("bulk threshold pins the boundary", () => {
  expect(isBulk(100)).toBe(true);
});
