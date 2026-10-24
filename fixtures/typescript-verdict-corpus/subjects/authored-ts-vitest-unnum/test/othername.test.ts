import { expect, test } from "vitest";
import { formatCents } from "../src/othername";

test("formats cents", () => {
  expect(formatCents(1250)).toBe("12.50");
});
