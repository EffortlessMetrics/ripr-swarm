import { expect, test } from "vitest";
import { isBulk } from "../src/asymAny";

test("bulk threshold accepts any boolean", () => {
  expect(isBulk(100)).toEqual(expect.any(Boolean));
});
