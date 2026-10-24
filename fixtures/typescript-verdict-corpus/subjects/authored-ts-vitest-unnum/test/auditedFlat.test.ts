import { expect, test } from "vitest";
import { chargeFlat } from "../src/auditedFlat";

test("charge takes the flat amount at the threshold", () => {
  expect(chargeFlat(100, 100)).toBe(90);
});
