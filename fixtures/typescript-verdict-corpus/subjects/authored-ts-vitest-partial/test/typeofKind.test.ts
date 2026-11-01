import { expect, test } from "vitest";
import { isBulk } from "../src/typeofKind";

test("bulk threshold returns a boolean", () => {
  expect(typeof isBulk(100)).toBe("boolean");
});
